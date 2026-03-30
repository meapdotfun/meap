use std::sync::Arc;
use std::collections::HashMap;

use crate::agent::{Agent, AgentId, Policy};
use crate::cluster::Cluster;
use crate::mesa::{MesaDetector, MesaReport};
use crate::signal::Signal;
use crate::tensor::Tensor;

/// Population-based self-play trainer for emergent communication protocols.
///
/// Agents are paired in cooperative-competitive tasks where they must
/// develop a shared communication protocol to coordinate effectively.
/// The training follows a curriculum:
///
/// Phase 1 (gen 0-50):     Random exploration, reward for basic coordination
/// Phase 2 (gen 50-200):   Signal differentiation pressure via mutual info bonus
/// Phase 3 (gen 200-800):  Compositionality pressure via systematic generalization tests
/// Phase 4 (gen 800+):     Protocol stabilization via cross-play evaluation
pub struct SelfPlayTrainer {
    population: Vec<Agent>,
    archive: Vec<ArchivedAgent>,
    mesa_detector: MesaDetector,
    config: SelfPlayConfig,
    generation: u64,
    metrics_log: Vec<GenerationMetrics>,
}

pub struct SelfPlayConfig {
    pub population_size: usize,
    pub episodes_per_gen: usize,
    pub tournament_size: usize,
    pub mutation_rate: f64,
    pub crossover_rate: f64,
    pub elite_fraction: f64,
    pub mesa_check_interval: u64,
    pub archive_threshold: f64,
    pub curriculum: CurriculumConfig,
}

pub struct CurriculumConfig {
    pub phase2_generation: u64,
    pub phase3_generation: u64,
    pub phase4_generation: u64,
    pub mi_bonus_coeff: f64,
    pub compositionality_coeff: f64,
    pub stability_coeff: f64,
}

struct ArchivedAgent {
    policy: Policy,
    generation: u64,
    fitness: f64,
    protocol_fingerprint: Vec<f64>,
}

pub struct GenerationMetrics {
    pub generation: u64,
    pub mean_fitness: f64,
    pub max_fitness: f64,
    pub mean_signal_entropy: f64,
    pub protocol_stability: f64,
    pub compositionality_score: f64,
    pub mesa_flags: usize,
    pub unique_protocols: usize,
}

impl SelfPlayTrainer {
    pub fn new(config: SelfPlayConfig) -> Self {
        let population: Vec<Agent> = (0..config.population_size)
            .map(|i| Agent::random(AgentId(i as u64)))
            .collect();

        let mesa_detector = MesaDetector::new(
            &population[0].policy,
            Default::default(),
        );

        Self {
            population,
            archive: Vec::new(),
            mesa_detector,
            config,
            generation: 0,
            metrics_log: Vec::new(),
        }
    }

    /// Run one generation of self-play training.
    pub fn step(&mut self) -> GenerationMetrics {
        let phase = self.current_phase();

        // 1. evaluate all agents via round-robin pairing
        let fitnesses = self.evaluate_population(phase);

        // 2. compute auxiliary metrics
        let signal_entropy = self.measure_signal_entropy();
        let compositionality = if phase >= 3 {
            self.measure_compositionality()
        } else {
            0.0
        };
        let stability = if phase >= 4 {
            self.measure_protocol_stability()
        } else {
            0.0
        };

        // 3. mesa-optimization detection
        let mesa_flags = if self.generation % self.config.mesa_check_interval == 0 {
            self.run_mesa_detection()
        } else {
            0
        };

        // 4. selection + reproduction
        let elite_count = (self.config.population_size as f64
            * self.config.elite_fraction) as usize;

        let mut ranked: Vec<(usize, f64)> = fitnesses.iter()
            .enumerate()
            .map(|(i, &f)| (i, f))
            .collect();
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        // archive top performers for historical cross-play
        if ranked[0].1 > self.config.archive_threshold {
            let best = &self.population[ranked[0].0];
            self.archive.push(ArchivedAgent {
                policy: best.policy.clone(),
                generation: self.generation,
                fitness: ranked[0].1,
                protocol_fingerprint: self.extract_protocol_fingerprint(best),
            });
        }

        let mut next_gen = Vec::with_capacity(self.config.population_size);

        // elites pass through unchanged
        for &(idx, _) in ranked.iter().take(elite_count) {
            next_gen.push(self.population[idx].clone());
        }

        // fill remaining slots via tournament selection + crossover/mutation
        while next_gen.len() < self.config.population_size {
            let parent_a = self.tournament_select(&fitnesses);
            let parent_b = self.tournament_select(&fitnesses);

            let mut child = if fastrand::f64() < self.config.crossover_rate {
                Policy::crossover(&parent_a.policy, &parent_b.policy)
            } else {
                parent_a.policy.clone()
            };

            if fastrand::f64() < self.config.mutation_rate {
                child.mutate(self.adaptive_mutation_scale());
            }

            next_gen.push(Agent::with_policy(
                AgentId(next_gen.len() as u64),
                child,
            ));
        }

        self.population = next_gen;
        self.generation += 1;

        let unique_protocols = self.count_unique_protocols();

        let metrics = GenerationMetrics {
            generation: self.generation,
            mean_fitness: fitnesses.iter().sum::<f64>() / fitnesses.len() as f64,
            max_fitness: ranked[0].1,
            mean_signal_entropy: signal_entropy,
            protocol_stability: stability,
            compositionality_score: compositionality,
            mesa_flags,
            unique_protocols,
        };

        self.metrics_log.push(metrics.clone());
        metrics
    }

    fn current_phase(&self) -> u32 {
        if self.generation >= self.config.curriculum.phase4_generation { 4 }
        else if self.generation >= self.config.curriculum.phase3_generation { 3 }
        else if self.generation >= self.config.curriculum.phase2_generation { 2 }
        else { 1 }
    }

    /// Evaluate all agents through structured pairing.
    ///
    /// Each agent plays `episodes_per_gen` episodes against randomly
    /// selected partners. Fitness includes the phase-specific auxiliary
    /// objectives (mutual information, compositionality, stability).
    fn evaluate_population(&self, phase: u32) -> Vec<f64> {
        let mut fitnesses = vec![0.0f64; self.population.len()];
        let mut counts = vec![0usize; self.population.len()];

        for _ in 0..self.config.episodes_per_gen {
            // sample random pairs
            let pairs = random_pairing(self.population.len());

            for (i, j) in pairs {
                let result = self.run_episode(&self.population[i], &self.population[j]);

                let mut reward_i = result.reward_a;
                let mut reward_j = result.reward_b;

                // phase-specific bonuses
                match phase {
                    2 => {
                        // mutual information bonus for signal differentiation
                        let mi = mutual_information(
                            &result.signals_a, &result.states,
                        );
                        reward_i += mi * self.config.curriculum.mi_bonus_coeff;
                        reward_j += mi * self.config.curriculum.mi_bonus_coeff;
                    }
                    3 => {
                        // compositionality bonus: reward systematic signal structure
                        let comp = compositionality_metric(
                            &result.signals_a, &result.signals_b,
                        );
                        reward_i += comp * self.config.curriculum.compositionality_coeff;
                        reward_j += comp * self.config.curriculum.compositionality_coeff;
                    }
                    4 => {
                        // stability bonus: reward consistent protocol use
                        let stab = self.protocol_consistency(
                            &self.population[i], &result.signals_a,
                        );
                        reward_i += stab * self.config.curriculum.stability_coeff;
                        reward_j += stab * self.config.curriculum.stability_coeff;
                    }
                    _ => {}
                }

                fitnesses[i] += reward_i;
                fitnesses[j] += reward_j;
                counts[i] += 1;
                counts[j] += 1;
            }
        }

        // normalize by episode count
        fitnesses.iter().zip(counts.iter()).map(|(&f, &c)| {
            if c > 0 { f / c as f64 } else { 0.0 }
        }).collect()
    }

    fn run_episode(&self, agent_a: &Agent, agent_b: &Agent) -> EpisodeResult {
        let mut env = CoordinationTask::new();
        let mut signals_a = Vec::new();
        let mut signals_b = Vec::new();
        let mut states = Vec::new();
        let mut total_reward_a = 0.0;
        let mut total_reward_b = 0.0;

        for step in 0..env.max_steps() {
            let state = env.observe();
            states.push(state.clone());

            let (action_a, signal_a) = agent_a.act(&state, &signals_b);
            let (action_b, signal_b) = agent_b.act(&state, &signals_a);

            signals_a.push(signal_a);
            signals_b.push(signal_b);

            let (reward_a, reward_b, done) = env.step(&action_a, &action_b);
            total_reward_a += reward_a;
            total_reward_b += reward_b;

            if done { break; }
        }

        EpisodeResult {
            reward_a: total_reward_a,
            reward_b: total_reward_b,
            signals_a,
            signals_b,
            states,
        }
    }

    fn tournament_select<'a>(&'a self, fitnesses: &[f64]) -> &'a Agent {
        let mut best_idx = fastrand::usize(..self.population.len());
        let mut best_fit = fitnesses[best_idx];

        for _ in 1..self.config.tournament_size {
            let idx = fastrand::usize(..self.population.len());
            if fitnesses[idx] > best_fit {
                best_idx = idx;
                best_fit = fitnesses[idx];
            }
        }

        &self.population[best_idx]
    }

    /// Mutation scale decreases as protocol stabilizes.
    fn adaptive_mutation_scale(&self) -> f64 {
        let base = 0.1;
        let decay = (-0.002 * self.generation as f64).exp();
        base * decay + 0.001 // floor to maintain exploration
    }

    fn run_mesa_detection(&mut self) -> usize {
        let mut flags = 0;
        for agent in &self.population {
            if self.mesa_detector.quick_check(agent) {
                let report = self.mesa_detector.scan(agent);
                if report.flagged {
                    flags += 1;
                    log::warn!(
                        "mesa-optimizer detected in agent {}: \
                         obj_div={:.4}, behavioral_gap={:.4}",
                        agent.id.0, report.objective_divergence, report.behavioral_gap,
                    );
                }
            }
        }
        flags
    }

    fn measure_signal_entropy(&self) -> f64 {
        let mut signal_counts: HashMap<u64, usize> = HashMap::new();
        let mut total = 0usize;

        for agent in &self.population {
            for signal in agent.recent_signals() {
                *signal_counts.entry(signal.hash()).or_insert(0) += 1;
                total += 1;
            }
        }

        if total == 0 { return 0.0; }

        signal_counts.values().map(|&count| {
            let p = count as f64 / total as f64;
            -p * p.ln()
        }).sum()
    }

    fn measure_compositionality(&self) -> f64 {
        // topographic similarity (Brighton & Kirby, 2006)
        // correlation between meaning-space distances and signal-space distances
        let sample_size = 100.min(self.population.len().pow(2));
        let mut meaning_dists = Vec::with_capacity(sample_size);
        let mut signal_dists = Vec::with_capacity(sample_size);

        for _ in 0..sample_size {
            let i = fastrand::usize(..self.population.len());
            let j = fastrand::usize(..self.population.len());
            if i == j { continue; }

            let fp_i = self.extract_protocol_fingerprint(&self.population[i]);
            let fp_j = self.extract_protocol_fingerprint(&self.population[j]);

            meaning_dists.push(l2_distance(&fp_i, &fp_j));

            let sig_i = self.population[i].signal_distribution();
            let sig_j = self.population[j].signal_distribution();
            signal_dists.push(jensen_shannon_vec(&sig_i, &sig_j));
        }

        pearson_correlation(&meaning_dists, &signal_dists).abs()
    }

    fn measure_protocol_stability(&self) -> f64 {
        if self.archive.is_empty() { return 0.0; }

        // cross-play with archived agents from previous generations
        let recent_archive: Vec<&ArchivedAgent> = self.archive.iter()
            .rev()
            .take(5)
            .collect();

        let mut cross_scores = Vec::new();
        for archived in &recent_archive {
            let ghost = Agent::with_policy(AgentId(u64::MAX), archived.policy.clone());
            for current in self.population.iter().take(8) {
                let result = self.run_episode(current, &ghost);
                cross_scores.push(result.reward_a + result.reward_b);
            }
        }

        let mean = cross_scores.iter().sum::<f64>() / cross_scores.len() as f64;
        let min = cross_scores.iter().cloned().fold(f64::INFINITY, f64::min);

        // stability = worst-case / mean (1.0 = perfectly stable)
        if mean > 0.0 { min / mean } else { 0.0 }
    }

    fn extract_protocol_fingerprint(&self, agent: &Agent) -> Vec<f64> {
        agent.policy.signal_embedding_weights()
            .iter()
            .take(32)
            .copied()
            .collect()
    }

    fn protocol_consistency(&self, agent: &Agent, signals: &[Signal]) -> f64 {
        let expected = agent.expected_signal_distribution();
        let observed = Signal::empirical_distribution(signals);
        1.0 - jensen_shannon_vec(&expected, &observed)
    }

    fn count_unique_protocols(&self) -> usize {
        let fingerprints: Vec<Vec<f64>> = self.population.iter()
            .map(|a| self.extract_protocol_fingerprint(a))
            .collect();

        // count clusters via simple distance threshold
        let mut visited = vec![false; fingerprints.len()];
        let mut clusters = 0;

        for i in 0..fingerprints.len() {
            if visited[i] { continue; }
            clusters += 1;
            visited[i] = true;
            for j in (i+1)..fingerprints.len() {
                if !visited[j] && l2_distance(&fingerprints[i], &fingerprints[j]) < 0.5 {
                    visited[j] = true;
                }
            }
        }

        clusters
    }

    /// Check if the population has converged on a stable protocol.
    pub fn is_converged(&self) -> bool {
        if self.metrics_log.len() < 50 { return false; }

        let recent: Vec<&GenerationMetrics> = self.metrics_log.iter()
            .rev()
            .take(50)
            .collect();

        let stability_trend: f64 = recent.iter()
            .map(|m| m.protocol_stability)
            .sum::<f64>() / 50.0;

        let fitness_variance: f64 = {
            let mean = recent.iter().map(|m| m.mean_fitness).sum::<f64>() / 50.0;
            recent.iter().map(|m| (m.mean_fitness - mean).powi(2)).sum::<f64>() / 50.0
        };

        stability_trend > 0.85 && fitness_variance < 0.01 && recent[0].unique_protocols <= 2
    }
}

struct EpisodeResult {
    reward_a: f64,
    reward_b: f64,
    signals_a: Vec<Signal>,
    signals_b: Vec<Signal>,
    states: Vec<Tensor>,
}

fn random_pairing(n: usize) -> Vec<(usize, usize)> {
    let mut indices: Vec<usize> = (0..n).collect();
    for i in (1..n).rev() {
        let j = fastrand::usize(..=i);
        indices.swap(i, j);
    }
    indices.chunks(2)
        .filter(|c| c.len() == 2)
        .map(|c| (c[0], c[1]))
        .collect()
}

fn mutual_information(signals: &[Signal], states: &[Tensor]) -> f64 {
    // discretize and compute MI via histogram estimator
    let bins = 16;
    let mut joint = vec![vec![0usize; bins]; bins];
    let n = signals.len().min(states.len());

    for i in 0..n {
        let s_bin = (signals[i].quantize(bins) as usize).min(bins - 1);
        let t_bin = (states[i].quantize(bins) as usize).min(bins - 1);
        joint[s_bin][t_bin] += 1;
    }

    let mut mi = 0.0;
    for si in 0..bins {
        for ti in 0..bins {
            let p_joint = joint[si][ti] as f64 / n as f64;
            if p_joint < 1e-10 { continue; }
            let p_s: f64 = (0..bins).map(|t| joint[si][t] as f64 / n as f64).sum();
            let p_t: f64 = (0..bins).map(|s| joint[s][ti] as f64 / n as f64).sum();
            if p_s < 1e-10 || p_t < 1e-10 { continue; }
            mi += p_joint * (p_joint / (p_s * p_t)).ln();
        }
    }
    mi
}

fn compositionality_metric(signals_a: &[Signal], signals_b: &[Signal]) -> f64 {
    // residual entropy: low residual = high compositionality
    let total_entropy = Signal::joint_entropy(signals_a, signals_b);
    let marginal_a = Signal::marginal_entropy(signals_a);
    let marginal_b = Signal::marginal_entropy(signals_b);
    let redundancy = marginal_a + marginal_b - total_entropy;
    redundancy / (marginal_a + marginal_b + 1e-10)
}

fn l2_distance(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b.iter()).map(|(x, y)| (x - y).powi(2)).sum::<f64>().sqrt()
}

fn jensen_shannon_vec(p: &[f64], q: &[f64]) -> f64 {
    let m: Vec<f64> = p.iter().zip(q.iter()).map(|(a, b)| (a + b) / 2.0).collect();
    0.5 * kl_vec(p, &m) + 0.5 * kl_vec(q, &m)
}

fn kl_vec(p: &[f64], q: &[f64]) -> f64 {
    p.iter().zip(q.iter())
        .filter(|(&pi, &qi)| pi > 1e-10 && qi > 1e-10)
        .map(|(&pi, &qi)| pi * (pi / qi).ln())
        .sum()
}

fn pearson_correlation(x: &[f64], y: &[f64]) -> f64 {
    let n = x.len() as f64;
    let mean_x = x.iter().sum::<f64>() / n;
    let mean_y = y.iter().sum::<f64>() / n;
    let cov: f64 = x.iter().zip(y.iter())
        .map(|(xi, yi)| (xi - mean_x) * (yi - mean_y))
        .sum::<f64>() / n;
    let std_x = (x.iter().map(|xi| (xi - mean_x).powi(2)).sum::<f64>() / n).sqrt();
    let std_y = (y.iter().map(|yi| (yi - mean_y).powi(2)).sum::<f64>() / n).sqrt();
    if std_x < 1e-10 || std_y < 1e-10 { return 0.0; }
    cov / (std_x * std_y)
}
