use tokio_fsm::{Transition, fsm};

#[fsm(initial = Idle, serde = true)]
impl SerdeFeatureRequiredFsm {
    type Context = ();
    type Error = std::convert::Infallible;

    #[on(state = Idle, event = Start)]
    async fn on_start(&mut self) -> Transition<Done> {
        Transition::to(Done)
    }
}

fn main() {}
