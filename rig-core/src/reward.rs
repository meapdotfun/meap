use crate::agent::AgentId;
use crate::cluster::Cluster;
use crate::signal::Signal;
use crate::tensor::Tensor;
use crate::world_model::WorldModel;

/// Multi-objective reward signal for agent communication policies.
///
/// The reward decomposes into three terms with learned mixing coefficients:
///
///   R(s, a, s') = α · I(s'; M) - β · C(a) + γ · Δcoherence(cluster)
///
/// where α, β, γ are optimized via a meta-gradient on the outer coordination
/// objective (see `MetaLearner`). This allows the reward landscape to adapt
/// as agents develop more sophisticated communication strategies.
pub struct RewardFunction {
    alpha: f64,
    beta: f64,
    gamma: f64,
    meta_learner: MetaLearner,
    config: RewardConfig,
}

pub struct RewardConfig {
    pub initial_alpha: f64,
    pub initial_beta: f64,
    pub initial_gamma: f64,
    pub meta_lr: f64,
    pub cost_model: CostModel,
    pub intrinsic_reward: IntrinsicRewardType,
}

impl Default for RewardConfig {
    fn default() -> Self {
        Self {
            initial_alpha: 1.0,
            initial_beta: 0.1,
            initial_gamma: 0.5,
            meta_lr: 1e-5,
            cost_model: CostModel::BandwidthLatency { bw_weight: 0.7, lat_weight: 0.3 },
            intrinsic_reward: IntrinsicRewardType::RND,
        }
    }
}

pub enum CostModel {
    /// Cost proportional to signal size
    Bandwidth { bytes_per_unit: f64 },
    /// Cost proportional to round-trip latency
    Latency { ms_per_unit: f64 },
    /// Weighted combination
    BandwidthLatency { bw_weight: f64, lat_weight: f64 },
    /// No communication cost (ablation baseline)
    Free,
}

pub enum IntrinsicRewardType {
    /// Random Network Distillation (Burda et al., 2018)
    RND,
    /// Prediction error on world model
    Curiosity,
    /// Count-based exploration via locality-sensitive hashing
    CountBased,
    /// No intrinsic reward
    None,
}

pub struct RewardComponents {
    pub info_gain: f64,
    pub communication_cost: f64,
    pub coherence_delta: f64,
    pub intrinsic: f64,
    pub total: f64,
}

impl RewardFunction {
    pub fn new(config: RewardConfig) -> Self {
        Self {
            alpha: config.initial_alpha,
            beta: config.initial_beta,
            gamma: config.initial_gamma,
            meta_learner: MetaLearner::new(config.meta_lr),
            config,
        }
    }

    /// Compute the full reward signal for a transition.
    pub fn compute(
        &self,
        state: &Tensor,
        action: &AgentAction,
        next_state: &Tensor,
        world_model: &WorldModel,
        cluster: Option<&Cluster>,
    ) -> RewardComponents {
        // term 1: information gain from the action's effect on world model
        let info_gain = self.information_gain(state, next_state, world_model);

        // term 2: communication cost
        let comm_cost = self.communication_cost(action);

        // term 3: cluster coherence change
        let coherence_delta = match cluster {
            Some(c) => self.coherence_delta(c, next_state),
            None => 0.0,
        };

        // optional intrinsic reward for exploration
        let intrinsic = self.intrinsic_reward(state, action, next_state);

        let total = self.alpha * info_gain
            - self.beta * comm_cost
            + self.gamma * coherence_delta
            + intrinsic;

        RewardComponents {
            info_gain,
            communication_cost: comm_cost,
            coherence_delta,
            intrinsic,
            total,
        }
    }

    /// Information gain: mutual information between next state and world model.
    ///
    /// Approximated as the reduction in world model uncertainty after
    /// observing the transition. High info gain = the agent's action
    /// produced an informative state change.
    fn information_gain(
        &self,
        state: &Tensor,
        next_state: &Tensor,
        world_model: &WorldModel,
    ) -> f64 {
        let prior = world_model.predict(state, &Tensor::zeros(1), &[]);
        let prior_entropy = prior.entropy();

        // posterior: condition on the observed next state
        let posterior_entropy = world_model
            .predict(next_state, &Tensor::zeros(1), &[])
            .entropy();

        (prior_entropy - posterior_entropy).max(0.0)
    }

    /// Communication cost based on the configured cost model.
    fn communication_cost(&self, action: &AgentAction) -> f64 {
        let signal = match &action.signal {
            Some(s) => s,
            None => return 0.0,
        };

        match &self.config.cost_model {
            CostModel::Bandwidth { bytes_per_unit } => {
                signal.size_bytes() as f64 * bytes_per_unit
            }
            CostModel::Latency { ms_per_unit } => {
                signal.estimated_latency_ms() * ms_per_unit
            }
            CostModel::BandwidthLatency { bw_weight, lat_weight } => {
                let bw = signal.size_bytes() as f64 / 1024.0; // normalize to KB
                let lat = signal.estimated_latency_ms() / 100.0; // normalize to 100ms
                bw * bw_weight + lat * lat_weight
            }
            CostModel::Free => 0.0,
        }
    }

    /// Change in cluster coherence after an action.
    ///
    /// Coherence is measured as the KL divergence between the cluster's
    /// joint belief and a factored (independent) baseline. High coherence
    /// means agents in the cluster share aligned world models.
    fn coherence_delta(&self, cluster: &Cluster, next_state: &Tensor) -> f64 {
        let current = cluster.coherence();

        // simulate the cluster state after incorporating next_state
        let projected = cluster.project_coherence(next_state);

        projected - current
    }

    /// Intrinsic reward for exploration.
    fn intrinsic_reward(
        &self,
        state: &Tensor,
        action: &AgentAction,
        next_state: &Tensor,
    ) -> f64 {
        match &self.config.intrinsic_reward {
            IntrinsicRewardType::RND => {
                // random network distillation: reward = prediction error
                // of a learned network trying to match a fixed random network
                let target = self.meta_learner.rnd_target.forward(next_state);
                let prediction = self.meta_learner.rnd_predictor.forward(next_state);
                (target - prediction).pow(2.0).mean_scalar() * 0.01
            }
            IntrinsicRewardType::Curiosity => {
                // forward dynamics prediction error
                let predicted = self.meta_learner.forward_model
                    .predict(state, &action.as_tensor());
                (predicted - next_state).pow(2.0).mean_scalar() * 0.01
            }
            IntrinsicRewardType::CountBased => {
                // pseudo-count via LSH
                let hash = locality_sensitive_hash(next_state, 32);
                let count = self.meta_learner.state_counts
                    .get(&hash)
                    .copied()
                    .unwrap_or(0) as f64;
                1.0 / (count + 1.0).sqrt()
            }
            IntrinsicRewardType::None => 0.0,
        }
    }

    /// Update mixing coefficients via meta-gradient.
    ///
    /// The outer objective measures coordination efficiency over a batch
    /// of trajectories. The meta-gradient flows through the inner policy
    /// optimization to update α, β, γ.
    pub fn meta_step(&mut self, trajectories: &[Trajectory]) {
        let grad = self.meta_learner.compute_gradient(
            trajectories,
            self.alpha,
            self.beta,
            self.gamma,
        );

        self.alpha = (self.alpha + self.config.meta_lr * grad.d_alpha).max(0.01);
        self.beta = (self.beta + self.config.meta_lr * grad.d_beta).max(0.001);
        self.gamma = (self.gamma + self.config.meta_lr * grad.d_gamma).max(0.01);

        // log coefficient drift for monitoring
        log::debug!(
            "reward coefficients: α={:.4}, β={:.4}, γ={:.4}",
            self.alpha, self.beta, self.gamma,
        );
    }

    pub fn coefficients(&self) -> (f64, f64, f64) {
        (self.alpha, self.beta, self.gamma)
    }
}

pub struct MetaLearner {
    lr: f64,
    rnd_target: FixedRandomNet,
    rnd_predictor: TrainableNet,
    forward_model: ForwardDynamicsNet,
    state_counts: HashMap<u64, usize>,
}

pub struct MetaGradient {
    pub d_alpha: f64,
    pub d_beta: f64,
    pub d_gamma: f64,
}

impl MetaLearner {
    fn new(lr: f64) -> Self {
        Self {
            lr,
            rnd_target: FixedRandomNet::new(128, 64),
            rnd_predictor: TrainableNet::new(128, 64),
            forward_model: ForwardDynamicsNet::new(128, 32),
            state_counts: HashMap::new(),
        }
    }

    /// Compute meta-gradient via truncated backprop through time.
    ///
    /// For each trajectory:
    ///   1. Compute inner loss (policy gradient) under current α, β, γ
    ///   2. Simulate one inner optimization step
    ///   3. Evaluate the adapted policy on the outer objective
    ///   4. Backprop through steps 1-3 to get d(outer)/d(α, β, γ)
    fn compute_gradient(
        &self,
        trajectories: &[Trajectory],
        alpha: f64,
        beta: f64,
        gamma: f64,
    ) -> MetaGradient {
        let eps = 1e-4;

        // finite-difference approximation of the meta-gradient
        // (cheaper than full TBPTT, sufficient for smooth objectives)
        let base_outer = self.outer_objective(trajectories, alpha, beta, gamma);

        let d_alpha = (self.outer_objective(trajectories, alpha + eps, beta, gamma)
            - base_outer) / eps;
        let d_beta = (self.outer_objective(trajectories, alpha, beta + eps, gamma)
            - base_outer) / eps;
        let d_gamma = (self.outer_objective(trajectories, alpha, beta, gamma + eps)
            - base_outer) / eps;

        MetaGradient { d_alpha, d_beta, d_gamma }
    }

    fn outer_objective(
        &self,
        trajectories: &[Trajectory],
        alpha: f64,
        beta: f64,
        gamma: f64,
    ) -> f64 {
        // coordination efficiency: total coordination reward / total cost
        trajectories.iter().map(|traj| {
            let coord_reward: f64 = traj.rewards.iter()
                .map(|r| alpha * r.info_gain + gamma * r.coherence_delta)
                .sum();
            let total_cost: f64 = traj.rewards.iter()
                .map(|r| beta * r.communication_cost)
                .sum();
            if total_cost > 0.0 { coord_reward / total_cost } else { coord_reward }
        }).sum::<f64>() / trajectories.len() as f64
    }
}

pub struct Trajectory {
    pub states: Vec<Tensor>,
    pub actions: Vec<AgentAction>,
    pub rewards: Vec<RewardComponents>,
    pub signals: Vec<Signal>,
}

pub struct AgentAction {
    pub movement: Tensor,
    pub signal: Option<Signal>,
}

impl AgentAction {
    fn as_tensor(&self) -> Tensor {
        self.movement.clone()
    }
}

fn locality_sensitive_hash(state: &Tensor, num_planes: usize) -> u64 {
    let mut hash = 0u64;
    for i in 0..num_planes {
        // random hyperplane (deterministic from index)
        let plane = Tensor::random_normal_seeded(state.len(), i as u64);
        if state.dot(&plane) > 0.0 {
            hash |= 1 << i;
        }
    }
    hash
}

use std::collections::HashMap;
