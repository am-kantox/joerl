//! Example demonstrating GenServer deferred reply pattern.
//!
//! This example shows how to use `CallResponse::NoReply` to defer replies
//! while performing async work, similar to Erlang's `{noreply, State}` and
//! `gen_server:reply/2`.
//!
//! Run with: cargo run --example gen_server_deferred_reply

use async_trait::async_trait;
use joerl::{
    ActorSystem,
    gen_server::{self, CallResponse, GenServer, GenServerContext},
};
use std::sync::Arc;
use std::time::Duration;

/// A job processor that performs async work and replies later
struct JobProcessor;

#[derive(Debug, Clone)]
enum Job {
    /// Quick job that replies immediately
    Quick(String),
    /// Slow job that defers reply and does async work
    Slow(String),
    /// Job that performs external API call
    FetchData(String),
}

#[derive(Debug)]
enum Command {
    GetStats,
}

#[derive(Debug, Clone)]
struct JobResult {
    job_id: String,
    result: String,
    processing_time_ms: u64,
}

struct Stats {
    quick_jobs: u32,
    slow_jobs: u32,
    fetch_jobs: u32,
}

#[async_trait]
impl GenServer for JobProcessor {
    type State = Stats;
    type Call = Job;
    type Cast = Command;
    type CallReply = JobResult;

    async fn init(&mut self, _ctx: &mut GenServerContext<'_, Self>) -> Self::State {
        println!("🚀 JobProcessor started");
        Stats {
            quick_jobs: 0,
            slow_jobs: 0,
            fetch_jobs: 0,
        }
    }

    async fn handle_call(
        &mut self,
        job: Self::Call,
        state: &mut Self::State,
        ctx: &mut GenServerContext<'_, Self>,
    ) -> CallResponse<Self::CallReply> {
        match job {
            Job::Quick(job_id) => {
                // Quick jobs reply immediately
                state.quick_jobs += 1;
                println!("⚡ Quick job {}: processing immediately", job_id);

                CallResponse::Reply(JobResult {
                    job_id: job_id.clone(),
                    result: format!("Quick result for {}", job_id),
                    processing_time_ms: 0,
                })
            }

            Job::Slow(job_id) => {
                // Slow jobs defer reply and process asynchronously
                state.slow_jobs += 1;
                println!("🐌 Slow job {}: deferring reply for async work", job_id);

                let reply_handle = ctx.reply_handle();

                // Spawn async task to do the work
                tokio::spawn(async move {
                    let start = tokio::time::Instant::now();

                    // Simulate expensive computation
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    let result = format!("Computed result for {}", job_id);

                    let elapsed = start.elapsed().as_millis() as u64;

                    // Send reply when work is done
                    reply_handle
                        .reply(JobResult {
                            job_id: job_id.clone(),
                            result,
                            processing_time_ms: elapsed,
                        })
                        .expect("Failed to send reply");

                    println!("✅ Slow job {} completed after {}ms", job_id, elapsed);
                });

                // Return immediately without replying
                CallResponse::NoReply
            }

            Job::FetchData(url) => {
                // External API calls also defer reply
                state.fetch_jobs += 1;
                println!("🌐 Fetch job for {}: deferring reply for API call", url);

                let reply_handle = ctx.reply_handle();

                tokio::spawn(async move {
                    let start = tokio::time::Instant::now();

                    // Simulate API call
                    tokio::time::sleep(Duration::from_millis(300)).await;
                    let data = format!("Data from {}", url);

                    let elapsed = start.elapsed().as_millis() as u64;

                    reply_handle
                        .reply(JobResult {
                            job_id: url.clone(),
                            result: data,
                            processing_time_ms: elapsed,
                        })
                        .expect("Failed to send reply");

                    println!("✅ Fetch job {} completed after {}ms", url, elapsed);
                });

                CallResponse::NoReply
            }
        }
    }

    async fn handle_cast(
        &mut self,
        cmd: Self::Cast,
        state: &mut Self::State,
        _ctx: &mut GenServerContext<'_, Self>,
    ) {
        match cmd {
            Command::GetStats => {
                println!(
                    "📊 Stats - Quick: {}, Slow: {}, Fetch: {}",
                    state.quick_jobs, state.slow_jobs, state.fetch_jobs
                );
            }
        }
    }
}

#[tokio::main]
async fn main() {
    println!("=== GenServer Deferred Reply Example ===\n");

    let system = Arc::new(ActorSystem::new());
    let processor = gen_server::spawn(&system, JobProcessor);

    // Submit various jobs
    println!("Submitting jobs...\n");

    // Quick job - replies immediately
    let quick_result = processor
        .call(Job::Quick("Q1".to_string()))
        .await
        .expect("Quick job failed");
    println!(
        "Got quick result: {} ({}ms)\n",
        quick_result.result, quick_result.processing_time_ms
    );

    // Slow jobs - deferred replies
    let slow_job1 = tokio::spawn({
        let processor = processor.clone();
        async move {
            processor
                .call(Job::Slow("S1".to_string()))
                .await
                .expect("Slow job 1 failed")
        }
    });

    let slow_job2 = tokio::spawn({
        let processor = processor.clone();
        async move {
            processor
                .call(Job::Slow("S2".to_string()))
                .await
                .expect("Slow job 2 failed")
        }
    });

    // Fetch job
    let fetch_job = tokio::spawn({
        let processor = processor.clone();
        async move {
            processor
                .call(Job::FetchData("https://api.example.com/data".to_string()))
                .await
                .expect("Fetch job failed")
        }
    });

    // While waiting, the processor can still handle other requests
    tokio::time::sleep(Duration::from_millis(100)).await;
    processor.cast(Command::GetStats).await.unwrap();

    // Another quick job while slow jobs are processing
    let quick_result2 = processor
        .call(Job::Quick("Q2".to_string()))
        .await
        .expect("Quick job 2 failed");
    println!(
        "\nGot quick result during slow processing: {} ({}ms)\n",
        quick_result2.result, quick_result2.processing_time_ms
    );

    // Wait for all deferred replies
    let slow_result1 = slow_job1.await.unwrap();
    let slow_result2 = slow_job2.await.unwrap();
    let fetch_result = fetch_job.await.unwrap();

    println!("\n=== All Jobs Completed ===");
    println!(
        "Slow job 1: {} ({}ms)",
        slow_result1.result, slow_result1.processing_time_ms
    );
    println!(
        "Slow job 2: {} ({}ms)",
        slow_result2.result, slow_result2.processing_time_ms
    );
    println!(
        "Fetch job: {} ({}ms)",
        fetch_result.result, fetch_result.processing_time_ms
    );

    // Final stats
    println!();
    processor.cast(Command::GetStats).await.unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;
}
