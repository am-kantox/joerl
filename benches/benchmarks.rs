use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use joerl::{Actor, ActorContext, ActorSystem, ExitReason, Message};
use joerl::gen_server::{GenServer, GenServerContext};
use joerl::gen_statem::{GenStatem, StateMachineContext, StateTransition};
use joerl::supervisor::{ChildSpec, RestartStrategy, SupervisorSpec, spawn_supervisor};
use async_trait::async_trait;
use std::hint::black_box;
use std::sync::Arc;
use tokio::runtime::Runtime;

// ============================================================================
// Simple Actor
// ============================================================================

struct Counter {
    count: i32,
}

#[async_trait]
impl Actor for Counter {
    async fn handle_message(&mut self, msg: Message, _ctx: &mut ActorContext) {
        if let Some(cmd) = msg.downcast_ref::<&str>() {
            match *cmd {
                "increment" => self.count += 1,
                "decrement" => self.count -= 1,
                _ => {}
            }
        }
    }
}

// ============================================================================
// GenServer
// ============================================================================

struct CounterServer;

#[derive(Debug)]
enum CounterCall { Get, #[allow(dead_code)] Add(i32) }

#[derive(Debug)]
enum CounterCast { Increment }

#[async_trait]
impl GenServer for CounterServer {
    type State = i32;
    type Call = CounterCall;
    type Cast = CounterCast;
    type CallReply = i32;

    async fn init(&mut self, _ctx: &mut GenServerContext<'_, Self>) -> Self::State {
        0
    }

    async fn handle_call(
        &mut self,
        call: Self::Call,
        state: &mut Self::State,
        _ctx: &mut GenServerContext<'_, Self>,
    ) -> Self::CallReply {
        match call {
            CounterCall::Get => *state,
            CounterCall::Add(n) => {
                *state += n;
                *state
            }
        }
    }

    async fn handle_cast(
        &mut self,
        _cast: Self::Cast,
        state: &mut Self::State,
        _ctx: &mut GenServerContext<'_, Self>,
    ) {
        *state += 1;
    }
}

// ============================================================================
// GenStatem
// ============================================================================

struct TurnstileStatem;

#[derive(Debug, Clone, PartialEq)]
enum TurnstileState {
    Locked,
    Unlocked,
}

#[derive(Debug)]
enum TurnstileEvent {
    Coin,
    Push,
}

#[async_trait]
impl GenStatem for TurnstileStatem {
    type State = TurnstileState;
    type Data = (usize, usize);
    type Event = TurnstileEvent;
    type Reply = bool;

    async fn init(&mut self, _ctx: &mut StateMachineContext<'_, Self>) -> (Self::State, Self::Data) {
        (TurnstileState::Locked, (0, 0))
    }

    async fn handle_event(
        &mut self,
        event: Self::Event,
        state: Self::State,
        data: &mut Self::Data,
        _ctx: &mut StateMachineContext<'_, Self>,
    ) -> StateTransition<Self> {
        match (state, event) {
            (TurnstileState::Locked, TurnstileEvent::Coin) => {
                data.0 += 1;
                StateTransition::next_state(TurnstileState::Unlocked, true)
            }
            (TurnstileState::Locked, TurnstileEvent::Push) => {
                StateTransition::keep_state(false)
            }
            (TurnstileState::Unlocked, TurnstileEvent::Coin) => {
                data.0 += 1;
                StateTransition::keep_state(true)
            }
            (TurnstileState::Unlocked, TurnstileEvent::Push) => {
                data.1 += 1;
                StateTransition::next_state(TurnstileState::Locked, true)
            }
        }
    }
}

// ============================================================================
// Supervisor Worker
// ============================================================================

struct WorkerActor {
    #[allow(dead_code)]
    id: usize,
}

#[async_trait]
impl Actor for WorkerActor {
    async fn handle_message(&mut self, msg: Message, ctx: &mut ActorContext) {
        if let Some(cmd) = msg.downcast_ref::<&str>() {
            if *cmd == "crash" {
                ctx.stop(ExitReason::Panic("crash".to_string()));
            }
        }
    }
}

// ============================================================================
// Benchmarks
// ============================================================================

fn bench_actor_spawn(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    c.bench_function("actor_spawn", |b| {
        b.iter(|| {
            rt.block_on(async {
                let system = ActorSystem::new();
                let _actor = system.spawn(Counter { count: 0 });
                black_box(());
            });
        });
    });
}

fn bench_actor_messages(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("actor_messages");
    
    for count in [10, 100, 1000].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(count), count, |b, &count| {
            b.iter(|| {
                rt.block_on(async {
                    let system = ActorSystem::new();
                    let actor = system.spawn(Counter { count: 0 });
                    
                    for _ in 0..count {
                        actor.send(Box::new("increment")).await.unwrap();
                    }
                    
                    tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;
                    black_box(());
                });
            });
        });
    }
    group.finish();
}

fn bench_gen_server(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    c.bench_function("gen_server_calls", |b| {
        b.iter(|| {
            rt.block_on(async {
                let system = Arc::new(ActorSystem::new());
                let server = joerl::gen_server::spawn(&system, CounterServer);
                
                for _ in 0..10 {
                    let _ = server.call(CounterCall::Get).await.unwrap();
                }
                
                black_box(());
            });
        });
    });
}

fn bench_gen_server_casts(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    c.bench_function("gen_server_casts", |b| {
        b.iter(|| {
            rt.block_on(async {
                let system = Arc::new(ActorSystem::new());
                let server = joerl::gen_server::spawn(&system, CounterServer);
                
                for _ in 0..10 {
                    server.cast(CounterCast::Increment).await.unwrap();
                }
                
                tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;
                black_box(());
            });
        });
    });
}

fn bench_gen_statem(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    c.bench_function("gen_statem_transitions", |b| {
        b.iter(|| {
            rt.block_on(async {
                let system = Arc::new(ActorSystem::new());
                let statem = joerl::gen_statem::spawn(&system, TurnstileStatem);
                
                for _ in 0..10 {
                    statem.call(TurnstileEvent::Coin).await.unwrap();
                    statem.call(TurnstileEvent::Push).await.unwrap();
                }
                
                black_box(());
            });
        });
    });
}

fn bench_supervisor(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("supervisor");
    
    for count in [5, 10, 20].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(count), count, |b, &count| {
            b.iter(|| {
                rt.block_on(async {
                    let system = Arc::new(ActorSystem::new());
                    let mut spec = SupervisorSpec::new(RestartStrategy::OneForOne);
                    
                    for i in 0..count {
                        let id = i;
                        spec = spec.child(ChildSpec::new(
                            format!("worker_{}", id),
                            move || Box::new(WorkerActor { id })
                        ));
                    }
                    
                    let _sup = spawn_supervisor(&system, spec);
                    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
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
    bench_actor_messages,
    bench_gen_server,
    bench_gen_server_casts,
    bench_gen_statem,
    bench_supervisor,
);

criterion_main!(benches);
