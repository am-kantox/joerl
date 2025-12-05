// Comparative benchmarks: joerl vs actix
//
// This benchmark suite compares joerl (Erlang-inspired) with actix (production actor framework)
// on equivalent operations to provide performance insights.

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use std::hint::black_box;

// ============================================================================
// joerl implementations
// ============================================================================

mod joerl_impl {
    use joerl::{Actor, ActorContext, Message};
    use async_trait::async_trait;

    pub struct Counter {
        pub count: i32,
    }

    #[async_trait]
    impl Actor for Counter {
        async fn handle_message(&mut self, msg: Message, _ctx: &mut ActorContext) {
            if let Some(n) = msg.downcast_ref::<i32>() {
                self.count += n;
            }
        }
    }

    pub struct PingActor {
        pub count: usize,
    }

    #[async_trait]
    impl Actor for PingActor {
        async fn handle_message(&mut self, msg: Message, _ctx: &mut ActorContext) {
            if let Some(_) = msg.downcast_ref::<usize>() {
                self.count += 1;
            }
        }
    }
}

// ============================================================================
// actix implementations  
// ============================================================================

mod actix_impl {
    use actix::prelude::*;

    #[derive(Message)]
    #[rtype(result = "()")]
    pub struct AddMsg(pub i32);

    pub struct Counter {
        pub count: i32,
    }

    impl Actor for Counter {
        type Context = Context<Self>;
    }

    impl Handler<AddMsg> for Counter {
        type Result = ();

        fn handle(&mut self, msg: AddMsg, _ctx: &mut Context<Self>) {
            self.count += msg.0;
        }
    }

    #[derive(Message)]
    #[rtype(result = "()")]
    pub struct PingMsg(#[allow(dead_code)] pub usize);

    pub struct PingActor {
        pub count: usize,
    }

    impl Actor for PingActor {
        type Context = Context<Self>;
    }

    impl Handler<PingMsg> for PingActor {
        type Result = ();

        fn handle(&mut self, _msg: PingMsg, _ctx: &mut Context<Self>) {
            self.count += 1;
        }
    }
}

// ============================================================================
// Benchmarks
// ============================================================================

fn bench_actor_spawn(c: &mut Criterion) {
    use actix::Actor as _; // Required for .start()
    let mut group = c.benchmark_group("actor_spawn");
    
    // joerl
    group.bench_function("joerl", |b| {
        let rt = tokio::runtime::Runtime::new().unwrap();
        b.iter(|| {
            rt.block_on(async {
                let system = joerl::ActorSystem::new();
                let _actor = system.spawn(joerl_impl::Counter { count: 0 });
                black_box(());
            });
        });
    });

    // actix
    group.bench_function("actix", |b| {
        b.iter(|| {
            let sys = actix::System::new();
            sys.block_on(async {
                let _addr = actix_impl::Counter { count: 0 }.start();
                black_box(());
            });
        });
    });
    
    group.finish();
}

fn bench_message_send(c: &mut Criterion) {
    use actix::Actor as _;
    let mut group = c.benchmark_group("message_send");
    
    for count in [10, 100, 1000].iter() {
        // joerl
        group.bench_with_input(BenchmarkId::new("joerl", count), count, |b, &count| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            b.iter(|| {
                rt.block_on(async {
                    let system = joerl::ActorSystem::new();
                    let actor = system.spawn(joerl_impl::Counter { count: 0 });
                    
                    for _ in 0..count {
                        actor.send(Box::new(1i32)).await.unwrap();
                    }
                    
                    tokio::time::sleep(tokio::time::Duration::from_millis(2)).await;
                    black_box(());
                });
            });
        });

        // actix
        group.bench_with_input(BenchmarkId::new("actix", count), count, |b, &count| {
            b.iter(|| {
                let sys = actix::System::new();
                sys.block_on(async {
                    let addr = actix_impl::Counter { count: 0 }.start();
                    
                    for _ in 0..count {
                        addr.do_send(actix_impl::AddMsg(1));
                    }
                    
                    tokio::time::sleep(tokio::time::Duration::from_millis(2)).await;
                    black_box(());
                });
            });
        });
    }
    
    group.finish();
}

fn bench_actor_throughput(c: &mut Criterion) {
    use actix::Actor as _;
    let mut group = c.benchmark_group("throughput");
    group.sample_size(20); // Fewer samples for throughput tests
    
    let message_count = 1000;
    
    // joerl
    group.bench_function("joerl_1000_msgs", |b| {
        let rt = tokio::runtime::Runtime::new().unwrap();
        b.iter(|| {
            rt.block_on(async {
                let system = joerl::ActorSystem::new();
                let actor = system.spawn(joerl_impl::PingActor { count: 0 });
                
                for _ in 0..message_count {
                    actor.send(Box::new(1usize)).await.unwrap();
                }
                
                tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                black_box(());
            });
        });
    });

    // actix
    group.bench_function("actix_1000_msgs", |b| {
        b.iter(|| {
            let sys = actix::System::new();
            sys.block_on(async {
                let addr = actix_impl::PingActor { count: 0 }.start();
                
                for _ in 0..message_count {
                    addr.do_send(actix_impl::PingMsg(1));
                }
                
                tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                black_box(());
            });
        });
    });
    
    group.finish();
}

fn bench_multiple_actors(c: &mut Criterion) {
    use actix::Actor as _;
    let mut group = c.benchmark_group("multiple_actors");
    
    for count in [10, 50].iter() {
        // joerl
        group.bench_with_input(BenchmarkId::new("joerl", count), count, |b, &count| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            b.iter(|| {
                rt.block_on(async {
                    let system = joerl::ActorSystem::new();
                    let mut actors = Vec::new();
                    
                    for _ in 0..count {
                        actors.push(system.spawn(joerl_impl::Counter { count: 0 }));
                    }
                    
                    for actor in &actors {
                        actor.send(Box::new(1i32)).await.unwrap();
                    }
                    
                    tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;
                    black_box(());
                });
            });
        });

        // actix
        group.bench_with_input(BenchmarkId::new("actix", count), count, |b, &count| {
            b.iter(|| {
                let sys = actix::System::new();
                sys.block_on(async {
                    let mut addrs = Vec::new();
                    
                    for _ in 0..count {
                        addrs.push(actix_impl::Counter { count: 0 }.start());
                    }
                    
                    for addr in &addrs {
                        addr.do_send(actix_impl::AddMsg(1));
                    }
                    
                    tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;
                    black_box(());
                });
            });
        });
    }
    
    group.finish();
}

criterion_group!(
    benches,
    bench_actor_spawn,
    bench_message_send,
    bench_actor_throughput,
    bench_multiple_actors,
);

criterion_main!(benches);
