//! Document Workflow FSM Example
//!
//! This example demonstrates a more complex state machine for document approval workflow:
//!
//! ## States
//! - draft: Initial state, document being written
//! - review: Document submitted for review
//! - approved: Document approved by reviewer
//! - rejected: Document rejected, needs changes
//! - published: Document published (terminal state)
//!
//! ## Events  
//! - submit: Submit draft for review
//! - approve: Approve the document
//! - reject: Reject and request changes
//! - revise: Make requested changes
//! - publish: Publish approved document
//! - archive: Archive published document (terminates)
//!
//! ## FSM Diagram
//!
//! ```text
//!     [*] --> draft
//!     draft --submit--> review
//!     review --approve--> approved
//!     review --reject--> rejected
//!     rejected --revise--> draft
//!     approved --publish--> published
//!     published --archive--> [*]
//! ```

use joerl::{ActorSystem, ExitReason, gen_statem};
use std::sync::Arc;

#[gen_statem(fsm = r#"
    [*] --> draft
    draft --> |submit| review
    review --> |approve| approved
    review --> |reject| rejected
    rejected --> |revise| draft
    approved --> |publish| published
    published --> |archive| [*]
"#)]
#[derive(Debug, Clone)]
struct Document {
    title: String,
    version: u32,
    approver: Option<String>,
    revision_count: u32,
}

impl Document {
    fn on_transition(
        &mut self,
        event: DocumentEvent,
        state: DocumentState,
    ) -> DocumentTransitionResult {
        match (state.clone(), event.clone()) {
            (DocumentState::Draft, DocumentEvent::Submit) => {
                println!("📝 Submitting document '{}' for review", self.title);
                self.version += 1;
                DocumentTransitionResult::Next(DocumentState::Review, self.clone())
            }
            (DocumentState::Review, DocumentEvent::Approve) => {
                println!("✅ Document approved");
                self.approver = Some("Reviewer".to_string());
                DocumentTransitionResult::Next(DocumentState::Approved, self.clone())
            }
            (DocumentState::Review, DocumentEvent::Reject) => {
                println!("❌ Document rejected - needs changes");
                self.revision_count += 1;
                DocumentTransitionResult::Next(DocumentState::Rejected, self.clone())
            }
            (DocumentState::Rejected, DocumentEvent::Revise) => {
                println!("🔧 Revising document (revision #{})", self.revision_count);
                DocumentTransitionResult::Next(DocumentState::Draft, self.clone())
            }
            (DocumentState::Approved, DocumentEvent::Publish) => {
                println!("🚀 Publishing document to production");
                DocumentTransitionResult::Next(DocumentState::Published, self.clone())
            }
            (DocumentState::Published, DocumentEvent::Archive) => {
                println!("📦 Archiving document");
                // Terminal transition - FSM will stop
                DocumentTransitionResult::Keep(self.clone())
            }
            _ => {
                println!("⚠️  Invalid transition: {:?} -> {:?}", state, event);
                DocumentTransitionResult::Error(format!(
                    "Cannot {:?} from state {:?}",
                    event, state
                ))
            }
        }
    }

    fn on_enter(&self, old_state: &DocumentState, new_state: &DocumentState, _data: &DocumentData) {
        println!(
            "  🔄 State: {:?} → {:?} (v{})",
            old_state, new_state, self.version
        );
    }

    fn on_terminate(&self, reason: &ExitReason, state: &DocumentState, data: &DocumentData) {
        println!("\n═══════════════════════════════════");
        println!("📊 Workflow Statistics");
        println!("═══════════════════════════════════");
        println!("  Document: {}", data.title);
        println!("  Final Version: {}", data.version);
        println!("  Revisions: {}", data.revision_count);
        println!("  Approver: {}", data.approver.as_deref().unwrap_or("None"));
        println!("  Final State: {:?}", state);
        println!("  Exit Reason: {:?}", reason);
        println!("═══════════════════════════════════\n");
    }
}

#[tokio::main]
async fn main() {
    println!("\n╔══════════════════════════════════════════╗");
    println!("║  Document Workflow FSM (gen_statem)     ║");
    println!("╚══════════════════════════════════════════╝\n");

    println!("States: draft → review → approved → published");
    println!("Reject path: review → rejected → draft\n");
    println!("──────────────────────────────────────────\n");

    let system = Arc::new(ActorSystem::new());

    let initial_data = Document {
        title: "Quarterly Report Q4 2024".to_string(),
        version: 0,
        approver: None,
        revision_count: 0,
    };

    let doc = Document(&system, initial_data);

    // === Scenario: Draft → Review → Reject → Revise → Review → Approve → Publish → Archive ===

    println!("📄 Starting workflow for: Quarterly Report Q4 2024\n");

    // Submit for first review
    println!("[1] Submit for review");
    doc.send(Box::new(DocumentEvent::Submit)).await.unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    println!();

    // Reject - needs changes
    println!("[2] Reviewer rejects document");
    doc.send(Box::new(DocumentEvent::Reject)).await.unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    println!();

    // Revise the document
    println!("[3] Author revises document");
    doc.send(Box::new(DocumentEvent::Revise)).await.unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    println!();

    // Re-submit for review
    println!("[4] Resubmit for review");
    doc.send(Box::new(DocumentEvent::Submit)).await.unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    println!();

    // Approve
    println!("[5] Reviewer approves document");
    doc.send(Box::new(DocumentEvent::Approve)).await.unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    println!();

    // Publish
    println!("[6] Publish to production");
    doc.send(Box::new(DocumentEvent::Publish)).await.unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    println!();

    // Archive (terminal)
    println!("[7] Archive document");
    doc.send(Box::new(DocumentEvent::Archive)).await.unwrap();

    // Give time for termination callback
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    println!("✅ Workflow completed successfully\n");
    println!("Key Features:");
    println!("  • Multi-step approval workflow");
    println!("  • Rejection and revision cycle");
    println!("  • Version tracking across transitions");
    println!("  • Terminal state with statistics\n");
}
