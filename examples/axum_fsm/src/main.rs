use std::{collections::HashMap, sync::Arc};

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::{
    sync::Mutex,
    time::{Duration, sleep},
};
use tokio_fsm::{Transition, fsm};

// --- DOMAIN TYPES ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub id: String,
    pub items: Vec<String>,
    pub total: u64,
}

#[derive(Debug)]
pub struct OrderContext {
    pub order: Order,
}

// --- FSM DEFINITION ---

#[fsm(initial = Created, tracing = true, serde = true)]
impl OrderFsm {
    type Context = OrderContext;
    type Error = std::convert::Infallible;

    // 1. Created -> Validated
    #[on(state = Created, event = Validate)]
    async fn handle_validate(&mut self) -> Transition<Validated> {
        tracing::info!(id = %self.context.order.id, "Validating order...");
        // Simulate validation logic
        sleep(Duration::from_millis(100)).await;
        tracing::debug!(id = %self.context.order.id, "Order validated");
        Transition::to(Validated)
    }

    // 2. Validated -> Charged
    #[on(state = Validated, event = Charge)]
    async fn handle_charge(&mut self) -> Transition<Charged> {
        tracing::info!(id = %self.context.order.id, "Charging order...");
        // Simulate payment processing
        sleep(Duration::from_millis(200)).await;
        tracing::debug!(id = %self.context.order.id, "Payment successful");
        Transition::to(Charged)
    }

    // 3. Charged -> Shipped
    #[on(state = Charged, event = Ship)]
    async fn handle_ship(&mut self) -> Transition<Shipped> {
        tracing::info!(id = %self.context.order.id, "Shipping order...");
        // Simulate shipping logic
        sleep(Duration::from_millis(300)).await;
        tracing::debug!(id = %self.context.order.id, "Order shipped");
        Transition::to(Shipped)
    }

    // Error handling transitions (simplified for demo)
    #[on(state = Created, event = Error)]
    #[on(state = Validated, event = Error)]
    #[on(state = Charged, event = Error)]
    async fn handle_error(&mut self) -> Transition<Failed> {
        tracing::error!("Order {} failed", self.context.order.id);
        Transition::to(Failed)
    }
}

// --- API STATE & ERRORS ---

#[derive(Debug)]
enum AppError {
    NotFound,
    FsmClosed,
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        let (status, message) = match self {
            AppError::NotFound => (StatusCode::NOT_FOUND, "Order not found"),
            AppError::FsmClosed => (StatusCode::GONE, "Order FSM is already closed"),
        };
        (status, Json(json!({ "error": message }))).into_response()
    }
}

struct AppState {
    // Map of OrderID -> (FSM Handle, FSM Task)
    orders: Mutex<HashMap<String, (OrderFsmHandle, OrderFsmTask)>>,
}

impl AppState {
    async fn send_event(&self, id: &str, event: OrderFsmEvent) -> Result<(), AppError> {
        let handle = {
            let orders = self.orders.lock().await;
            orders.get(id).map(|(h, _)| h.clone())
        }
        .ok_or(AppError::NotFound)?;

        handle.send(event).await.map_err(|_| AppError::FsmClosed)
    }
}

// --- AXUM HANDLERS ---

#[derive(Deserialize)]
struct CreateOrderRequest {
    id: String,
    items: Vec<String>,
    total: u64,
}

async fn create_order(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateOrderRequest>,
) -> impl IntoResponse {
    let order = Order {
        id: payload.id.clone(),
        items: payload.items,
        total: payload.total,
    };

    let context = OrderContext { order };
    let (handle, task) = OrderFsm::spawn_named(&payload.id, context);

    state
        .orders
        .lock()
        .await
        .insert(payload.id.clone(), (handle, task));

    (
        StatusCode::CREATED,
        Json(json!({ "status": "Order created" })),
    )
}

async fn validate_order(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    state.send_event(&id, OrderFsmEvent::Validate).await?;
    Ok((
        StatusCode::OK,
        Json(json!({ "status": "Validation started" })),
    ))
}

async fn charge_order(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    state.send_event(&id, OrderFsmEvent::Charge).await?;
    Ok((
        StatusCode::OK,
        Json(json!({ "status": "Charging started" })),
    ))
}

async fn ship_order(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    state.send_event(&id, OrderFsmEvent::Ship).await?;
    Ok((
        StatusCode::OK,
        Json(json!({ "status": "Shipping started" })),
    ))
}

async fn get_order_status(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    let orders = state.orders.lock().await;
    let (handle, _) = orders.get(&id).ok_or(AppError::NotFound)?;
    let state = handle.current_state();
    Ok((StatusCode::OK, Json(json!({ "state": state }))))
}

async fn stop_order(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    let mut orders = state.orders.lock().await;
    let (handle, task) = orders.remove(&id).ok_or(AppError::NotFound)?;

    handle.shutdown();
    let _ = task.await; // Wait for the FSM task to finish gracefully

    Ok((
        StatusCode::OK,
        Json(json!({ "status": "Order stopped and cleaned up" })),
    ))
}

// --- MAIN ---

#[tokio::main]
async fn main() {
    // 1. Initialize tokio-console and stdout logging
    // Requires RUSTFLAGS="--cfg tokio_unstable"
    use tracing_subscriber::prelude::*;

    tracing_subscriber::registry()
        .with(
            console_subscriber::ConsoleLayer::builder()
                .with_default_env()
                .spawn(),
        )
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    tracing::info!("Starting Axum FSM Server...");

    let app_state = Arc::new(AppState {
        orders: Mutex::new(HashMap::new()),
    });

    let app = Router::new()
        .route("/orders", post(create_order))
        .route("/orders/:id/validate", post(validate_order))
        .route("/orders/:id/charge", post(charge_order))
        .route("/orders/:id/ship", post(ship_order))
        .route("/orders/:id/stop", post(stop_order))
        .route("/orders/:id", get(get_order_status))
        .with_state(app_state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    tracing::info!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
