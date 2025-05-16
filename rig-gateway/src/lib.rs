use axum::{
    routing::{get, post},
    Router,
    extract::{Path, State, Json},
    http::{Request, StatusCode},
    response::Response,
    body::Body,
};
use rig_broker::Broker;
use rig_registry::{ServiceRegistry, ServiceInstance};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;
use tracing::{info, error};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct GatewayState {
    registry: Arc<ServiceRegistry>,
    broker: Arc<Broker>,
    routes: Arc<RwLock<Vec<Route>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Route {
    pub path: String,
    pub service: String,
    pub method: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceRequest {
    pub service: String,
    pub path: String,
    pub method: String,
    pub body: Option<serde_json::Value>,
}

impl GatewayState {
    pub fn new(registry: ServiceRegistry, broker: Broker) -> Self {
        Self {
            registry: Arc::new(registry),
            broker: Arc::new(broker),
            routes: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn add_route(&self, route: Route) {
        let mut routes = self.routes.write().await;
        routes.push(route);
    }

    pub async fn find_service(&self, path: &str, method: &str) -> Option<String> {
        let routes = self.routes.read().await;
        routes
            .iter()
            .find(|r| r.path == path && r.method == method)
            .map(|r| r.service.clone())
    }
}

pub async fn create_gateway(registry: ServiceRegistry, broker: Broker) -> Router {
    let state = Arc::new(GatewayState::new(registry, broker));

    Router::new()
        .route("/gateway/routes", post(add_route))
        .route("/gateway/routes", get(list_routes))
        .route("/gateway/services", get(list_services))
        .route("/*path", get(handle_request).post(handle_request))
        .layer(ServiceBuilder::new().layer(TraceLayer::new_for_http()))
        .with_state(state)
}

async fn add_route(
    State(state): State<Arc<GatewayState>>,
    Json(route): Json<Route>,
) -> StatusCode {
    state.add_route(route).await;
    StatusCode::CREATED
}

async fn list_routes(
    State(state): State<Arc<GatewayState>>,
) -> Json<Vec<Route>> {
    let routes = state.routes.read().await;
    Json(routes.clone())
}

async fn list_services(
    State(state): State<Arc<GatewayState>>,
) -> Json<Vec<ServiceInstance>> {
    let services = state.registry.get_instances("").await;
    Json(services)
}

async fn handle_request(
    State(state): State<Arc<GatewayState>>,
    Path(path): Path<String>,
    method: String,
    body: Option<Json<serde_json::Value>>,
) -> Response<Body> {
    if let Some(service_name) = state.find_service(&path, &method).await {
        if let Some(instance) = state.registry.get_healthy_instances(&service_name).await.first() {
            let url = format!("http://{}:{}{}", instance.host, instance.port, path);
            
            let client = reqwest::Client::new();
            let mut request = client.request(
                reqwest::Method::from_bytes(method.as_bytes()).unwrap(),
                &url,
            );

            if let Some(body) = body {
                request = request.json(&body.0);
            }

            match request.send().await {
                Ok(response) => {
                    let status = response.status();
                    let headers = response.headers().clone();
                    let body = response.bytes().await.unwrap_or_default();

                    let mut response_builder = Response::builder()
                        .status(status);

                    for (key, value) in headers.iter() {
                        response_builder = response_builder.header(key, value);
                    }

                    response_builder
                        .body(Body::from(body))
                        .unwrap_or_else(|_| {
                            Response::builder()
                                .status(StatusCode::INTERNAL_SERVER_ERROR)
                                .body(Body::from("Internal Server Error"))
                                .unwrap()
                        })
                }
                Err(e) => {
                    error!("Failed to forward request: {}", e);
                    Response::builder()
                        .status(StatusCode::BAD_GATEWAY)
                        .body(Body::from("Bad Gateway"))
                        .unwrap()
                }
            }
        } else {
            Response::builder()
                .status(StatusCode::SERVICE_UNAVAILABLE)
                .body(Body::from("Service Unavailable"))
                .unwrap()
        }
    } else {
        Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("Not Found"))
            .unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_gateway_routes() {
        let registry = ServiceRegistry::new();
        let broker = Broker::new("nats://localhost:4222").await.unwrap();
        let app = create_gateway(registry, broker).await;

        // Test adding a route
        let route = Route {
            path: "/test".to_string(),
            service: "test-service".to_string(),
            method: "GET".to_string(),
        };

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/gateway/routes")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&route).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CREATED);

        // Test listing routes
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/gateway/routes")
                    .method("GET")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }
} 