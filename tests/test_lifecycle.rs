use tokio::time::{Duration, sleep};
use tokio_fsm::{Transition, fsm};

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
