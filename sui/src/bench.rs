// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/*** How-to 
 * subcommand: `microbench` to run micro benchmarks
 * args:
 *      running_mode: 
 *          local-single-validator-thread: 
 *              start a validator in a different thread.
 *          local-single-validator-process: 
 *              start a validator in a new local process. 
 *              --working-dir needs to be specified on this mode where a `validator` binary exists
 * 
 * Examples:
 * ./bench microbench local-single-validator-process --port=9555 throughput --num-transactions 10 --batch-size=1 --working-dir=$YOUR_WORKPLACE/sui/target/debug
 * ./bench microbench local-single-validator-process latency --num-chunks 10 --batch-size 10 --working-dir=$YOUR_WORKPLACE/sui/target/debug
 * ./bench microbench local-single-validator-thread throughput --num-transactions 10 --batch-size=1
 * ./bench microbench local-single-validator-thread latency --num-chunks 10 --batch-size 10 
 * 
*/

#![deny(warnings)]

use clap::*;

use sui::benchmark::validator_preparer::VALIDATOR_BINARY_NAME;
use sui::benchmark::{bench_types, run_benchmark};
use tracing::subscriber::set_global_default;
use tracing_subscriber::EnvFilter;

fn main() {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let subscriber_builder =
        tracing_subscriber::fmt::Subscriber::builder().with_env_filter(env_filter);
    let subscriber = subscriber_builder.with_writer(std::io::stderr).finish();
    set_global_default(subscriber).expect("Failed to set subscriber");
    let benchmark = bench_types::Benchmark::parse();
    running_mode_pre_check(&benchmark);
    let r = run_benchmark(benchmark);
    println!("{}", r);
}

fn running_mode_pre_check(benchmark: &bench_types::Benchmark) {
    match benchmark.running_mode {
        bench_types::RunningMode::LocalSingleValidatorThread => {}
        bench_types::RunningMode::LocalSingleValidatorProcess => match &benchmark.working_dir {
            Some(path) => {
                assert!(
                    path.clone().join(VALIDATOR_BINARY_NAME).is_file(),
                    "validator binary needs to be in working-dir"
                );
            }
            None => panic!("working-dir option is required in local-single-authority-process mode"),
        },
    }
}
