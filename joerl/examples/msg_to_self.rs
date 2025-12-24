use async_trait::async_trait;
use joerl::{Actor, ActorContext, ActorSystem, ExitReason, Message};

struct SelfMessenger {
    count: u32,
    max: u32,
}

#[async_trait]
impl Actor for SelfMessenger {
    async fn started(&mut self, ctx: &mut ActorContext) {
        println!("[{}] Starting, sending first message to self", ctx.pid());
        // Send initial message to self
        ctx.send(ctx.pid(), Box::new("tick")).await.ok();
    }

    async fn handle_message(&mut self, msg: Message, ctx: &mut ActorContext) {
        if let Some(cmd) = msg.downcast_ref::<&str>()
            && *cmd == "tick"
        {
            self.count += 1;
            println!("[{}] Tick #{}", ctx.pid(), self.count);

            if self.count < self.max {
                // Send next message to self
                ctx.send(ctx.pid(), Box::new("tick")).await.ok();
            } else {
                println!("[{}] Max reached, stopping", ctx.pid());
                ctx.stop(ExitReason::Normal);
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let system = ActorSystem::new();
    let actor = system.spawn(SelfMessenger { count: 0, max: 5 });

    // Wait for the actor to finish
    let exit_reason = actor.join().await;
    println!("Done! Actor exited with: {}", exit_reason);
}
