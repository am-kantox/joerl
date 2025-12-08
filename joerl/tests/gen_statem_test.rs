//! Integration tests for gen_statem DSL

use joerl::{ActorSystem, ExitReason, gen_statem};
use std::sync::Arc;
use tokio::time::{Duration, sleep};

/// Simple traffic light FSM for testing
#[gen_statem(fsm = r#"
    [*] --> red
    red --> |timer| green
    green --> |timer| yellow
    yellow --> |timer| red
    red --> |shutdown| [*]
"#)]
#[derive(Debug, Clone)]
struct TrafficLight {
    cycles: u32,
}

impl TrafficLight {
    fn on_transition(
        &mut self,
        event: TrafficLightEvent,
        state: TrafficLightState,
    ) -> TrafficLightTransitionResult {
        match (state.clone(), event.clone()) {
            (TrafficLightState::Red, TrafficLightEvent::Timer) => {
                self.cycles += 1;
                TrafficLightTransitionResult::Next(TrafficLightState::Green, self.clone())
            }
            (TrafficLightState::Green, TrafficLightEvent::Timer) => {
                TrafficLightTransitionResult::Next(TrafficLightState::Yellow, self.clone())
            }
            (TrafficLightState::Yellow, TrafficLightEvent::Timer) => {
                TrafficLightTransitionResult::Next(TrafficLightState::Red, self.clone())
            }
            (TrafficLightState::Red, TrafficLightEvent::Shutdown) => {
                TrafficLightTransitionResult::Keep(self.clone())
            }
            _ => TrafficLightTransitionResult::Error(format!(
                "Invalid transition: {:?} with {:?}",
                state, event
            )),
        }
    }

    fn on_enter(
        &self,
        _old_state: &TrafficLightState,
        _new_state: &TrafficLightState,
        _data: &TrafficLightData,
    ) {
        // No-op for tests
    }

    fn on_terminate(
        &self,
        _reason: &ExitReason,
        _state: &TrafficLightState,
        _data: &TrafficLightData,
    ) {
        // No-op for tests
    }
}

#[tokio::test]
async fn test_gen_statem_basic_transitions() {
    let system = Arc::new(ActorSystem::new());
    let light = TrafficLight(&system, TrafficLight { cycles: 0 });

    // Test transitions: red -> green -> yellow -> red
    light
        .send(Box::new(TrafficLightEvent::Timer))
        .await
        .unwrap();
    sleep(Duration::from_millis(10)).await;

    light
        .send(Box::new(TrafficLightEvent::Timer))
        .await
        .unwrap();
    sleep(Duration::from_millis(10)).await;

    light
        .send(Box::new(TrafficLightEvent::Timer))
        .await
        .unwrap();
    sleep(Duration::from_millis(10)).await;

    // Actor should still be alive after cycling
    assert!(system.is_actor_alive(light.pid()));
}

#[tokio::test]
async fn test_gen_statem_terminal_transition() {
    let system = Arc::new(ActorSystem::new());
    let light = TrafficLight(&system, TrafficLight { cycles: 0 });

    // Shutdown should terminate the FSM
    light
        .send(Box::new(TrafficLightEvent::Shutdown))
        .await
        .unwrap();
    sleep(Duration::from_millis(50)).await;

    // Actor should be terminated
    assert!(!system.is_actor_alive(light.pid()));
}

#[tokio::test]
async fn test_gen_statem_state_data_preservation() {
    let system = Arc::new(ActorSystem::new());
    let light = TrafficLight(&system, TrafficLight { cycles: 0 });

    // Cycle through states multiple times
    for _ in 0..3 {
        light
            .send(Box::new(TrafficLightEvent::Timer))
            .await
            .unwrap();
        sleep(Duration::from_millis(10)).await;
    }

    // The cycles should have been tracked (we can't directly verify,
    // but the actor should still be functioning)
    assert!(system.is_actor_alive(light.pid()));
}

/// FSM with Keep transitions for testing
#[gen_statem(fsm = r#"
    [*] --> idle
    idle --> |activate| active
    active --> |ping| active
    active --> |deactivate| idle
"#)]
#[derive(Debug, Clone)]
struct PingPong {
    pings: u32,
}

impl PingPong {
    fn on_transition(
        &mut self,
        event: PingPongEvent,
        state: PingPongState,
    ) -> PingPongTransitionResult {
        match (state.clone(), event.clone()) {
            (PingPongState::Idle, PingPongEvent::Activate) => {
                PingPongTransitionResult::Next(PingPongState::Active, self.clone())
            }
            (PingPongState::Active, PingPongEvent::Ping) => {
                self.pings += 1;
                PingPongTransitionResult::Keep(self.clone())
            }
            (PingPongState::Active, PingPongEvent::Deactivate) => {
                PingPongTransitionResult::Next(PingPongState::Idle, self.clone())
            }
            _ => PingPongTransitionResult::Keep(self.clone()),
        }
    }

    fn on_enter(
        &self,
        _old_state: &PingPongState,
        _new_state: &PingPongState,
        _data: &PingPongData,
    ) {
    }

    fn on_terminate(&self, _reason: &ExitReason, _state: &PingPongState, _data: &PingPongData) {}
}

#[tokio::test]
async fn test_gen_statem_keep_transition() {
    let system = Arc::new(ActorSystem::new());
    let pinger = PingPong(&system, PingPong { pings: 0 });

    // Activate
    pinger
        .send(Box::new(PingPongEvent::Activate))
        .await
        .unwrap();
    sleep(Duration::from_millis(10)).await;

    // Send multiple pings (Keep transitions)
    for _ in 0..5 {
        pinger.send(Box::new(PingPongEvent::Ping)).await.unwrap();
        sleep(Duration::from_millis(5)).await;
    }

    // Should still be active
    assert!(system.is_actor_alive(pinger.pid()));

    // Deactivate
    pinger
        .send(Box::new(PingPongEvent::Deactivate))
        .await
        .unwrap();
    sleep(Duration::from_millis(10)).await;

    assert!(system.is_actor_alive(pinger.pid()));
}

#[tokio::test]
async fn test_gen_statem_invalid_transition_ignored() {
    let system = Arc::new(ActorSystem::new());
    let pinger = PingPong(&system, PingPong { pings: 0 });

    // Try to ping while idle (invalid)
    pinger.send(Box::new(PingPongEvent::Ping)).await.unwrap();
    sleep(Duration::from_millis(10)).await;

    // Actor should still be alive despite invalid transition
    assert!(system.is_actor_alive(pinger.pid()));
}
