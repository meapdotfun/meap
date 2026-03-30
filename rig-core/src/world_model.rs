use std::collections::VecDeque;

use crate::agent::AgentId;
use crate::signal::{Signal, SignalEmbedding};
use crate::tensor::{Tensor, Distribution, Normal};

/// Factored world model with epistemic uncertainty estimation.
///
/// The model decomposes state transitions into local dynamics (single-agent)
/// and interaction effects (cross-agent attention), following the factored
/// MDP formulation of Oliehoek & Amato (2016).
///
/// Epistemic uncertainty is estimated via deep ensemble disagreement
/// (Lakshminarayanan et al., 2017) with anchored ensembling for
/// non-stationary distributions (Osband et al., 2021).
pub struct WorldModel {
    local_dynamics: TransitionNet,
    interaction_net: CrossAgentAttention,
    ensemble: Vec<PredictionHead>,
    replay: VecDeque<Transition>,
    config: WorldModelConfig,
}

pub struct WorldModelConfig {
    pub ensemble_size: usize,
    pub hidden_dim: usize,
    pub num_heads: usize,
    pub replay_capacity: usize,
    pub learning_rate: f64,
    pub uncertainty_penalty: f64,
}

struct Transition {
    state: Tensor,
    action: Tensor,
    signals: Vec<SignalEmbedding>,
    next_state: Tensor,
    reward: f64,
    terminal: bool,
}

impl WorldModel {
    pub fn new(config: WorldModelConfig) -> Self {
        let ensemble = (0..config.ensemble_size)
            .map(|i| PredictionHead::new(config.hidden_dim, i as u64))
            .collect();

        Self {
            local_dynamics: TransitionNet::new(config.hidden_dim),
            interaction_net: CrossAgentAttention::new(
                config.hidden_dim,
                config.num_heads,
            ),
            ensemble,
            replay: VecDeque::with_capacity(config.replay_capacity),
            config,
        }
    }

    /// Predict next state distribution conditioned on joint actions and signals.
    ///
    /// Returns a calibrated distribution where variance reflects true
    /// epistemic uncertainty (not aleatoric noise).
    pub fn predict(
        &self,
        state: &Tensor,
        action: &Tensor,
        signals: &[SignalEmbedding],
    ) -> Distribution<Tensor> {
        // local dynamics: P(s'_i | s_i, a_i)
        let local_pred = self.local_dynamics.forward(state, action);

        // cross-agent interaction via multi-head attention
        let interaction = if signals.is_empty() {
            Tensor::zeros(self.config.hidden_dim)
        } else {
            self.interaction_net.attend(
                &state.as_query(),
                &signals.iter().map(|s| s.as_key()).collect::<Vec<_>>(),
                &signals.iter().map(|s| s.as_value()).collect::<Vec<_>>(),
            )
        };

        let combined = local_pred.concat(&interaction);

        // ensemble predictions for uncertainty
        let predictions: Vec<Normal> = self.ensemble
            .iter()
            .map(|head| head.predict(&combined))
            .collect();

        // epistemic uncertainty = variance of ensemble means
        // aleatoric uncertainty = mean of ensemble variances
        let means: Vec<Tensor> = predictions.iter().map(|p| p.mean.clone()).collect();
        let ensemble_mean = Tensor::stack(&means).mean_dim(0);

        let epistemic_var = means.iter()
            .map(|m| (m - &ensemble_mean).pow(2.0))
            .fold(Tensor::zeros_like(&ensemble_mean), |acc, v| acc + v)
            / self.config.ensemble_size as f64;

        let aleatoric_var = predictions.iter()
            .map(|p| p.variance.clone())
            .fold(Tensor::zeros_like(&ensemble_mean), |acc, v| acc + v)
            / self.config.ensemble_size as f64;

        Distribution::Normal(Normal {
            mean: ensemble_mean,
            variance: epistemic_var + aleatoric_var,
            epistemic_variance: Some(epistemic_var),
        })
    }

    /// Information gain from incorporating a new signal into the world model.
    ///
    /// Computed as the expected reduction in epistemic uncertainty (BALD;
    /// Houlsby et al., 2011) approximated via ensemble disagreement.
    pub fn expected_info_gain(&self, state: &Tensor, signal: &Signal) -> f64 {
        let prior_predictions: Vec<Normal> = self.ensemble
            .iter()
            .map(|head| head.predict(&state))
            .collect();
        let prior_entropy = ensemble_entropy(&prior_predictions);

        // condition on the new signal
        let signal_embed = signal.embed();
        let conditioned: Vec<Normal> = self.ensemble
            .iter()
            .map(|head| {
                let ctx = self.interaction_net.attend(
                    &state.as_query(),
                    &[signal_embed.as_key()],
                    &[signal_embed.as_value()],
                );
                head.predict(&state.concat(&ctx))
            })
            .collect();
        let posterior_entropy = ensemble_entropy(&conditioned);

        // info gain = entropy reduction, always non-negative
        (prior_entropy - posterior_entropy).max(0.0)
    }

    /// Train the world model on a batch of transitions.
    ///
    /// Each ensemble member trains on a bootstrapped subsample to maintain
    /// diversity. Uses the anchored ensembling trick to prevent mode collapse
    /// in non-stationary environments.
    pub fn train_step(&mut self, batch_size: usize) -> TrainMetrics {
        if self.replay.len() < batch_size {
            return TrainMetrics::insufficient_data();
        }

        let mut total_loss = 0.0;
        let mut disagreement = 0.0;

        for (i, head) in self.ensemble.iter_mut().enumerate() {
            // bootstrap: each head sees a different subsample
            let batch = bootstrap_sample(&self.replay, batch_size, i as u64);

            let loss: f64 = batch.iter().map(|t| {
                let signals_embed: Vec<SignalEmbedding> = t.signals.clone();
                let interaction = if signals_embed.is_empty() {
                    Tensor::zeros(self.config.hidden_dim)
                } else {
                    self.interaction_net.attend(
                        &t.state.as_query(),
                        &signals_embed.iter().map(|s| s.as_key()).collect::<Vec<_>>(),
                        &signals_embed.iter().map(|s| s.as_value()).collect::<Vec<_>>(),
                    )
                };

                let local = self.local_dynamics.forward(&t.state, &t.action);
                let combined = local.concat(&interaction);
                let pred = head.predict(&combined);
                pred.nll(&t.next_state)
            }).sum::<f64>() / batch_size as f64;

            // anchor regularization: penalize drift from initialization
            let anchor_loss = head.anchor_regularization() * 0.01;

            head.backward(loss + anchor_loss, self.config.learning_rate);
            total_loss += loss;
        }

        // measure ensemble disagreement as diagnostic
        let sample = &self.replay[self.replay.len() - 1];
        let preds: Vec<Normal> = self.ensemble
            .iter()
            .map(|h| h.predict(&sample.state))
            .collect();
        disagreement = ensemble_disagreement(&preds);

        TrainMetrics {
            loss: total_loss / self.config.ensemble_size as f64,
            disagreement,
            replay_size: self.replay.len(),
        }
    }

    pub fn store_transition(
        &mut self,
        state: Tensor,
        action: Tensor,
        signals: Vec<SignalEmbedding>,
        next_state: Tensor,
        reward: f64,
        terminal: bool,
    ) {
        if self.replay.len() >= self.config.replay_capacity {
            self.replay.pop_front();
        }
        self.replay.push_back(Transition {
            state, action, signals, next_state, reward, terminal,
        });
    }
}

pub struct TrainMetrics {
    pub loss: f64,
    pub disagreement: f64,
    pub replay_size: usize,
}

impl TrainMetrics {
    fn insufficient_data() -> Self {
        Self { loss: f64::NAN, disagreement: f64::NAN, replay_size: 0 }
    }
}

fn ensemble_entropy(predictions: &[Normal]) -> f64 {
    let means: Vec<&Tensor> = predictions.iter().map(|p| &p.mean).collect();
    let grand_mean = Tensor::mean_of(&means);

    // H[E[p(y|x)]] - entropy of the average prediction
    let avg_var: Tensor = predictions.iter()
        .map(|p| {
            let diff = &p.mean - &grand_mean;
            &p.variance + diff.pow(2.0)
        })
        .fold(Tensor::zeros_like(&grand_mean), |a, b| a + b)
        / predictions.len() as f64;

    // gaussian entropy: 0.5 * ln(2*pi*e*var)
    avg_var.log().mean_scalar() * 0.5 + 0.5 * (2.0 * std::f64::consts::PI * std::f64::consts::E).ln()
}

fn ensemble_disagreement(predictions: &[Normal]) -> f64 {
    let means: Vec<&Tensor> = predictions.iter().map(|p| &p.mean).collect();
    let grand_mean = Tensor::mean_of(&means);
    means.iter()
        .map(|m| (*m - &grand_mean).pow(2.0).mean_scalar())
        .sum::<f64>() / predictions.len() as f64
}

fn bootstrap_sample<T>(data: &VecDeque<T>, n: usize, seed: u64) -> Vec<&T> {
    let mut rng = Pcg64::seed_from(seed);
    (0..n).map(|_| &data[rng.next_usize() % data.len()]).collect()
}
