// SPDX-License-Identifier: GPL-3.0-or-later

//! `lushtext-journal-stateright [--two-processes] [--max-steps N]
//! [--threads N] [--strategy bfs|dfs] [--depth-bound] [--finish-when-k8]`
//!
//! `--finish-when-k8` stops as soon as both K8 properties (S1 and
//! NoBodyWithoutEntry) have counterexamples, instead of exploring the whole
//! bounded space; the state count is then partial.

use std::time::Instant;

use lushtext_journal_stateright::model::{JournalModel, State, Step};
use stateright::{Checker, HasDiscoveries, Model};

fn main() {
    let mut two_processes = false;
    let mut max_steps = 6;
    let mut threads = 1;
    let mut strategy = String::from("bfs");
    let mut depth_bound = false;
    let mut finish_when_k8 = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || args.next().unwrap_or_else(|| panic!("{arg} needs a value"));
        match arg.as_str() {
            "--two-processes" => two_processes = true,
            "--max-steps" => max_steps = value().parse().expect("--max-steps N"),
            "--threads" => threads = value().parse().expect("--threads N"),
            "--strategy" => strategy = value(),
            "--depth-bound" => depth_bound = true,
            "--finish-when-k8" => finish_when_k8 = true,
            other => panic!("unknown argument {other}"),
        }
    }
    let model = JournalModel {
        max_steps,
        two_processes,
        count_steps: !depth_bound,
    };
    if depth_bound {
        assert!(
            threads == 1 && strategy == "bfs",
            "--depth-bound is sound only with --threads 1 --strategy bfs"
        );
    }
    println!(
        "harness={} max_steps={max_steps} threads={threads} strategy={strategy} depth_bound={depth_bound} finish_when_k8={finish_when_k8}",
        if two_processes {
            "two-process (a_second_writer_breaks_the_journal_invariants)"
        } else {
            "one-process (journal_invariants_hold_under_crashes)"
        }
    );
    let started = Instant::now();
    let mut builder = model.clone().checker().threads(threads);
    if finish_when_k8 {
        builder = builder.finish_when(HasDiscoveries::AllOf(
            ["S1", "NoBodyWithoutEntry"].into_iter().collect(),
        ));
    }
    let checker: Box<dyn Checker<JournalModel>> = match strategy.as_str() {
        "bfs" => Box::new(builder.spawn_bfs().join()),
        "dfs" => Box::new(builder.spawn_dfs().join()),
        other => panic!("unknown strategy {other}"),
    };
    let elapsed = started.elapsed();
    println!("unique_states={}", checker.unique_state_count());
    println!("states_visited={}", checker.state_count());
    println!("max_depth={}", checker.max_depth());
    for property in model.properties() {
        match checker.discovery(property.name) {
            None => println!("{}: HOLDS", property.name),
            Some(path) => {
                println!("{}: VIOLATED; counterexample:", property.name);
                let steps: Vec<(State, Option<Step>)> = path.into_vec();
                let initial = &steps[0].0.journal;
                println!(
                    "  initial (after startup): entry={:?} body={:?} backing={:?} preserved={:#b}",
                    initial.entry.map(|e| e.present),
                    initial.body,
                    initial.backing,
                    initial.preserved
                );
                for (_, step) in steps.iter() {
                    if let Some(step) = step {
                        println!(
                            "  {} {:?} id={} fault={}{}",
                            if step.actor { "B" } else { "A" },
                            step.action,
                            step.id,
                            step.fault,
                            step.ending
                                .map(|e| format!(" ending={e:?}"))
                                .unwrap_or_default()
                        );
                    }
                }
            }
        }
    }
    println!("wall_seconds={:.3}", elapsed.as_secs_f64());
}
