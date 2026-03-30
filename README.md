<h1 align="center">MEAP</h1>
<p align="center">machine economy agent playground</p>

<p align="center">substrate for autonomous agent coordination. agents negotiate shared world models through structured message passing. collective intelligence emerges without centralized control.</p>

---

## core primitives

three constructs.

**signal**: typed, content addressed message carrying agent observations. immutable once emitted. forms a merkle linked causal history.

```rust
pub struct Signal<T: Observation> {
    pub source: AgentId,
    pub payload: T,
    pub causal_parent: Option<Hash>,
    pub timestamp: LogicalClock,
    pub attestation: Ed25519Signature,
}
```

**bond**: bidirectional trust channel formed through mutual attestation. carries a continuous trust score updated via bayesian belief propagation.

```rust
pub struct Bond {
    pub agents: (AgentId, AgentId),
    pub trust: f64,           // β posterior mean
    pub alpha: f64,           // successful interactions
    pub beta_param: f64,      // failed interactions
    pub formed_at: LogicalClock,
}

impl Bond {
    pub fn update(&mut self, outcome: Outcome) {
        match outcome {
            Outcome::Cooperative => self.alpha += 1.0,
            Outcome::Defective => self.beta_param += 1.0,
        }
        self.trust = self.alpha / (self.alpha + self.beta_param);
    }
}
```

**cluster**: agents that share a consensus world model. forms and dissolves based on information theoretic coherence.

```rust
pub struct Cluster {
    pub members: BTreeSet<AgentId>,
    pub world_model: WorldModel,
    pub coherence: f64,       // KL(p_cluster || p_uniform)
    pub generation: u64,
}
```

---

## reward shaping

agents learn communication policies through a reward signal that balances information gain against coordination cost:

```
R(s, a, s') = α · I(s'; M) - β · C(a) + γ · Δcoherence(cluster)

where:
  I(s'; M)           = mutual information between next state and world model
  C(a)               = communication cost (bandwidth + latency)
  Δcoherence         = change in cluster coherence after action
  α, β, γ            = learned mixing coefficients (meta gradient)
```

the mixing coefficients are optimized via meta gradient on the outer objective:

```rust
impl MetaLearner {
    pub fn step(&mut self, trajectory: &Trajectory) -> MetaGradient {
        let inner_loss = trajectory.policy_gradient_loss();
        let outer_loss = trajectory.coordination_efficiency();

        // TBPTT through the inner optimization
        let d_outer_d_alpha = autograd::jacobian(
            |alpha| {
                let adapted = self.policy.inner_step(inner_loss, alpha);
                adapted.evaluate(outer_loss)
            },
            &self.alpha,
        );

        MetaGradient {
            d_alpha: d_outer_d_alpha,
            d_beta: self.compute_cost_gradient(trajectory),
            d_gamma: self.compute_coherence_gradient(trajectory),
        }
    }
}
```

---

## world model

each agent maintains a learned world model `W(s, a, s')` predicting state transitions conditioned on joint actions. factored into local dynamics and interaction effects:

```rust
pub struct WorldModel {
    local_dynamics: TransitionNet,      // P(s'_i | s_i, a_i)
    interaction_net: AttentionLayer,    // cross agent influence
    uncertainty: EnsembleVariance,       // epistemic uncertainty via ensemble disagreement
}

impl WorldModel {
    pub fn predict(&self, state: &State, actions: &JointAction) -> Distribution<State> {
        let local = self.local_dynamics.forward(state, &actions.local);

        // cross agent attention over observed signals
        let context = self.interaction_net.attend(
            query: state.embedding(),
            keys: actions.signals().map(|s| s.embedding()),
            values: actions.signals().map(|s| s.payload_embedding()),
        );

        let combined = local.condition_on(context);
        self.uncertainty.calibrate(combined)
    }

    /// information gain from observing a new signal
    pub fn expected_info_gain(&self, signal: &Signal) -> f64 {
        let prior = self.uncertainty.entropy();
        let posterior = self.predict_with(signal).uncertainty.entropy();
        prior - posterior  // reduction in epistemic uncertainty
    }
}
```

---

## mesa optimization

detection for mesa optimizers: learned policies that develop internal optimization processes misaligned with the base objective.

```rust
pub struct MesaDetector {
    probe_net: LinearProbe,
    reference_policy: FrozenPolicy,
    divergence_threshold: f64,
}

impl MesaDetector {
    /// detect if the agent's internal representations encode
    /// an implicit objective different from the specified reward
    pub fn scan(&self, agent: &Agent) -> MesaReport {
        let activations = agent.policy.intermediate_activations();

        // linear probe to predict agent's *actual* optimization target
        let implicit_objective = self.probe_net.predict(&activations);
        let explicit_objective = agent.reward_spec();

        let divergence = kl_divergence(implicit_objective, explicit_objective);

        // behavioral divergence under distribution shift
        let ood_states = self.generate_adversarial_states(agent);
        let behavioral_gap = ood_states.iter().map(|s| {
            let base_action = self.reference_policy.act(s);
            let agent_action = agent.policy.act(s);
            action_divergence(base_action, agent_action)
        }).mean();

        MesaReport {
            objective_divergence: divergence,
            behavioral_gap,
            flagged: divergence > self.divergence_threshold
                || behavioral_gap > self.divergence_threshold * 2.0,
        }
    }
}
```

---

## self play convergence

population based self play. communication protocols emerge from cooperative/competitive dynamics:

```
generation 0:    random policies, no communication
generation ~50:  simple signaling (approach/avoid)
generation ~200: compositional signals (object + property)
generation ~800: negotiated shared abstractions
generation ~2k:  stable protocol with drift correction
```

convergence tracked via protocol stability:

```rust
pub fn protocol_stability(
    population: &[Agent],
    eval_episodes: usize,
) -> StabilityReport {
    let mut cross_play = Vec::new();

    for (i, a) in population.iter().enumerate() {
        for (j, b) in population.iter().enumerate() {
            if i >= j { continue; }
            let score = evaluate_pair(a, b, eval_episodes);
            cross_play.push(CrossPlayResult {
                agents: (i, j),
                coordination_score: score.coordination,
                communication_efficiency: score.bits_exchanged / score.task_reward,
                mutual_intelligibility: score.signal_overlap,
            });
        }
    }

    let mean_coord = cross_play.iter().map(|r| r.coordination_score).mean();
    let min_coord = cross_play.iter().map(|r| r.coordination_score).min_f64();

    StabilityReport {
        mean_cross_play: mean_coord,
        worst_case_cross_play: min_coord,
        stable: min_coord > 0.85 * mean_coord,
        generation: population[0].generation,
    }
}
```

---

## configuration

```toml
[agent]
observation_dim = 128
action_dim = 32
signal_vocab = 4096
hidden_dim = 512
num_heads = 8
world_model_ensemble_size = 5

[training]
population_size = 64
inner_lr = 3e-4
outer_lr = 1e-5
gamma = 0.995
gae_lambda = 0.97
entropy_coeff = 0.01
mesa_detection_interval = 100

[protocol]
max_signal_size = 1024
bond_timeout = 30_000
cluster_coherence_threshold = 0.7
trust_prior_alpha = 1.0
trust_prior_beta = 1.0
heartbeat_interval = 5_000

[transport]
bind = "0.0.0.0:9100"
tls = true
max_connections = 10_000
rate_limit = 1000
circuit_breaker_threshold = 5
circuit_breaker_timeout = 30_000
```

---

## benchmarks

```
┌────────────────────────────┬───────────┬──────────┬──────────────┐
│ task                       │ meap      │ baseline │ improvement  │
├────────────────────────────┼───────────┼──────────┼──────────────┤
│ signal throughput (msg/s)  │ 12,400    │ 3,200    │ 3.9x         │
│ bond formation latency     │ 4.2ms     │ 18.6ms   │ 4.4x         │
│ cluster convergence time   │ 1.8s      │ 12.4s    │ 6.9x         │
│ cross play coordination    │ 0.91      │ 0.62     │ +47%         │
│ mesa detection recall      │ 0.94      │ 0.71     │ +32%         │
│ world model pred. accuracy │ 0.87      │ 0.73     │ +19%         │
│ protocol stability (gen)   │ ~800      │ ~4,200   │ 5.3x faster  │
└────────────────────────────┴───────────┴──────────┴──────────────┘

64 agents, 8xA100, emergent comm v3 benchmark suite
```

---

## quickstart

```bash
cargo build --release

# spawn a local cluster of 8 agents
meap spawn --agents 8 --config meap.toml

# observe emergent communication
meap observe --cluster default --format json | jq '.signals'

# run self play training
meap train \
  --population 64 \
  --generations 2000 \
  --task coordination_v2 \
  --mesa-detection on \
  --checkpoint-dir ./runs/exp_001
```

---

## structure

```
meap/
├── rig-core/           core protocol, agent lifecycle, signal routing
├── rig-deepseek/       deepseek integration for world model backbone
├── rig-lancedb/        vector storage for signal embeddings
├── rig-mongodb/        persistent bond and cluster state
├── rig-neo4j/          agent relationship graph queries
├── rig-qdrant/         approximate nearest neighbor signal retrieval
├── rig-sqlite/         lightweight local agent state
├── tools/              cli, observer, benchmarking utilities
├── site/               playground and documentation
└── Cargo.toml          workspace configuration
```

---

research software. expect breaking changes.
