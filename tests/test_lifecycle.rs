use tokio::time::{Duration, sleep};
use tokio_fsm::{Transition, fsm};
use tokio_util::sync::CancellationToken;

#[fsm(initial = Idle)]
impl LifecycleFsm {
    type Context = ();
    type Error = std::convert::Infallible;

    #[on(state = Idle, event = Tick)]
    async fn on_tick(&mut self) -> Transition<Running> {
        Transition::to(Running)
    }
}

#[tokio::test]
async fn test_fsm_abort_on_drop() {
    let (handle, task) = LifecycleFsm::spawn(());

    // Send event to ensure it's running
    handle.send(LifecycleFsmEvent::Tick).await.unwrap();
    handle
        .wait_for_state(LifecycleFsmState::Running)
        .await
        .unwrap();

    // Drop the task handle - this should abort the FSM
    drop(task);

    // After a short delay, the receiver should be closed
    sleep(Duration::from_millis(10)).await;
    let res = handle.send(LifecycleFsmEvent::Tick).await;
    assert!(res.is_err(), "Expected send to fail after task is dropped");
}

#[tokio::test]
async fn test_fsm_manual_shutdown() {
    let (handle, task) = LifecycleFsm::spawn(());

    handle.shutdown();

    let res = task.await;
    assert!(res.is_ok(), "Task should return Ok on graceful shutdown");

    let res = handle.send(LifecycleFsmEvent::Tick).await;
    assert!(res.is_err(), "Expected send to fail after shutdown");
}

#[tokio::test]
async fn test_shutdown_does_not_cancel_parent_token() {
    let parent = CancellationToken::new();
    let (handle, task) = LifecycleFsm::spawn_with_token((), parent.clone());

    handle.shutdown();

    assert!(
        !parent.is_cancelled(),
        "handle.shutdown() must not cancel the caller's token"
    );
    assert!(
        task.await.is_ok(),
        "task should stop cleanly when the child token is cancelled"
    );
}

#[tokio::test]
async fn test_parent_token_cancels_fsm() {
    let parent = CancellationToken::new();
    let (_handle, task) = LifecycleFsm::spawn_with_token((), parent.clone());

    parent.cancel();

    assert!(
        task.await.is_ok(),
        "parent token cancellation should propagate to the FSM"
    );
}

pub struct BlockingContext {
    started_tx: Option<tokio::sync::oneshot::Sender<()>>,
    completed: bool,
}

#[fsm(initial = BlockingIdle)]
impl BlockingFsm {
    type Context = BlockingContext;
    type Error = std::convert::Infallible;

    #[on(state = BlockingIdle, event = Run)]
    async fn on_run(&mut self) -> Transition<BlockingDone> {
        if let Some(started_tx) = self.context.started_tx.take() {
            let _ = started_tx.send(());
        }

        tokio::time::sleep(Duration::from_secs(60)).await;
        self.context.completed = true;
        Transition::to(BlockingDone)
    }
}

#[tokio::test]
async fn test_shutdown_cancels_long_running_handler_promptly() {
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let context = BlockingContext {
        started_tx: Some(started_tx),
        completed: false,
    };
    let (handle, task) = BlockingFsm::spawn(context);

    handle.send(BlockingFsmEvent::Run).await.unwrap();
    started_rx.await.unwrap();

    handle.shutdown();

    let context = tokio::time::timeout(Duration::from_millis(100), task)
        .await
        .expect("shutdown should interrupt a blocked handler")
        .unwrap();
    assert!(
        !context.completed,
        "cancelled handler must not run to completion"
    );
}

#[cfg(feature = "tracing")]
#[fsm(initial = TracedIdle, tracing = true)]
impl TracedLifecycleFsm {
    type Context = ();
    type Error = std::convert::Infallible;

    #[on(state = TracedIdle, event = Tick)]
    async fn on_tick(&mut self) -> Transition<TracedDone> {
        Transition::to(TracedDone)
    }
}

#[cfg(feature = "tracing")]
#[tokio::test]
async fn test_tracing_enabled_fsm_runs() {
    let (handle, task) = TracedLifecycleFsm::spawn(());

    handle.send(TracedLifecycleFsmEvent::Tick).await.unwrap();
    handle
        .wait_for_state(TracedLifecycleFsmState::TracedDone)
        .await
        .unwrap();

    handle.shutdown();
    assert!(task.await.is_ok());
}
