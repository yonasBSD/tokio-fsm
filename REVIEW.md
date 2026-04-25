# tokio-fsm Production Readiness Review

## 1. Project Intent & Scope

- `tokio-fsm` is a compile-time FSM generator for Tokio that targets Rust developers who want typed async state machines without hand-writing channels/event loops.
- Scope is mostly well-defined for single-process in-memory FSM orchestration, but docs/examples position it near production workflows before reliability and diagnostics are fully hardened.
- Gap: project claims "zero overhead" and "production-grade lifecycle semantics", but benchmark methodology and some API/diagnostic edges are not yet strict enough for broad production trust.

## 2. Idiomatic Rust Analysis (HIGH PRIORITY)

### What is idiomatic

- Runtime core is clean and minimal (`src/core.rs`), especially `Transition<T>` and `TaskError<E>`.
- Public surface is compact (`src/lib.rs`) and uses feature-gated re-exports reasonably.

### Non-idiomatic / fragile patterns

1. **Invariant-dependent panics in proc macro code (non-idiomatic for macro diagnostics).** (fixed in current branch)
   - `tokio-fsm-macros/src/codegen/impls/run.rs` and `tokio-fsm-macros/src/validation/graph.rs` now propagate `syn::Error` instead of panic paths.
   - This improves compile-time diagnostics and avoids internal macro panics.

2. **Parser accepts ambiguous attribute combinations.** (fixed in current branch)
   - `tokio-fsm-macros/src/validation/parser.rs` now rejects methods that combine `#[on(...)]` and `#[on_timeout]`.
   - A dedicated `trybuild` UI test covers this rule.

3. **No upfront validation of handler signature shape.**
   - Parser validates return types, but not strict method form (`async fn`, expected receiver shape) before codegen.
   - Invalid forms likely fail in expanded code with noisy errors, not clear macro diagnostics.

### Before/After (critical idiomatic fix)

**Before**

```rust
// tokio-fsm-macros/src/codegen/impls/run.rs
handler
    .return_kind
    .expect("event handlers must have a parsed return kind")
```

**After**

```rust
// Convert to fallible codegen with syn::Result<TokenStream>
let return_kind = handler.return_kind.ok_or_else(|| {
    syn::Error::new_spanned(
        &handler.method.sig.ident,
        "internal macro error: missing parsed return kind for handler; report this as a bug"
    )
})?;
```

## 3. Memory & Performance Deep Dive (HIGH PRIORITY)

1. **Benchmark throughput case is methodologically incorrect.**
   - `benches/comparison.rs` sends only `Ping` in `throughput_macro_fire`.
   - FSM transitions `Idle -> Running` once; subsequent `Ping` events are mostly unmatched-event overhead, not transition throughput.
   - This can overstate/understate performance depending on branch behavior.

2. **Reachability validation is more expensive than needed.** (fixed in current branch)
   - `tokio-fsm-macros/src/validation/graph.rs` now uses a single DFS from the initial state, then checks membership for each node.
   - This removes repeated path searches and reduces validator overhead for larger FSMs.

3. **Avoidable allocations/scans in parser path.**
   - `tokio-fsm-macros/src/validation/parser.rs` repeatedly dedupes states with `Vec + iter().any(...)` and allocates strings for error construction.
   - This is not catastrophic but unnecessary; switch to `HashSet<Ident>` during collection.

## 4. Error Handling & Robustness

- **Good:** runtime exposes typed `TaskError<E>`; clear separation of logical FSM errors vs join failures.
- **Weak:** examples demonstrate error-erasure patterns:
  - `examples/worker.rs` maps DB errors to `Transition<Failed>` and discards the underlying cause with `map_err(|_| ...)`.
- **Weak:** runtime examples use `unwrap()` heavily in non-test async/network boundaries (`examples/axum_fsm/src/main.rs`, `examples/worker.rs`), which teaches panic-driven handling.

Recommendation:
- Keep library typed errors (`thiserror`) as-is.
- Improve examples to propagate context (`map_err(|e| ...)`) or log structured causes before state fallback.

## 5. Concurrency & Async

1. **Lock held across await in Axum example.** (fixed in current branch)
   - `stop_order` now removes `(handle, task)` under lock, then drops the guard before `task.await`.
   - This avoids serializing unrelated requests behind shutdown waits.

2. **Duplicate ID insertion leaks task ownership semantics.** (fixed in current branch)
   - `create_order` now rejects duplicates with `409 Conflict` (`AppError::AlreadyExists`) instead of overwriting an existing entry.
   - This preserves task ownership/lifecycle control in the in-memory map.

3. **Positive:** cancellation and lifecycle semantics are better than average and tested (`tests/test_lifecycle.rs`, `spawn_with_token` behavior).

### Before/After (async correctness fix)

**Before**

```rust
let mut orders = state.orders.lock().await;
let (handle, task) = orders.remove(&id).ok_or(AppError::NotFound)?;
handle.shutdown();
let _ = task.await;
```

**After**

```rust
let (handle, task) = {
    let mut orders = state.orders.lock().await;
    orders.remove(&id).ok_or(AppError::NotFound)?
};
handle.shutdown();
let _ = task.await;
```

## 6. Dependency & Build Hygiene

- Dependencies are generally reasonable and scoped.
- `edition = "2024"` and `rust-version = "1.92"` are aggressive for ecosystem adoption unless explicitly justified/documented.
- Release workflow is weaker than CI:
  - CI: strict clippy + workspace all-features tests.
  - Release: only `cargo test` (no workspace/all-features/clippy parity).
- Local `justfile` defaults to `cargo +nightly` for routine tasks, creating contributor friction.

## 7. API Design & Ergonomics

- API shape is Rusty: generated types (`*State`, `*Event`, `*Handle`, `*Task`) are intuitive.
- `#[must_use]` on transitions is correct and prevents silent misuse.
- Ergonomic gaps:
  - Macro diagnostics for invalid signatures/ambiguous attrs need explicit checks to avoid opaque expansion errors.
  - Examples should model non-panicking usage to set expectations for downstream developers.

## 8. Testing, Benchmarks & Reliability

- **Strong:** integration tests cover lifecycle, channel-close drain behavior, timeout behavior, cancellation semantics.
- **Strong:** proc-macro has `trybuild` compile-fail suite.
- **Missing / risky:**
  - Some reliability checks were improved (paused-time and polling-based waits), but more deterministic timing coverage can still be expanded.
  - Compile-fail coverage now includes mixed `#[on] + #[on_timeout]`; dedicated invalid-signature tests are still worth adding.
  - Benchmarks need correction for true transition throughput measurement.

## 9. Open Source Readiness (HIGH PRIORITY)

1. **Docs command mismatch in example README.**
   - Root workspace excludes `examples/axum_fsm`, but README says `cargo run -p axum_fsm`.
   - This is a first-run failure for new contributors.

2. **Community/process files are incomplete.**
   - Missing `CONTRIBUTING.md`, `SECURITY.md`, and `CODE_OF_CONDUCT.md`.

3. **CI/release inconsistency reduces trust.**
   - Release gate should enforce at least CI-equivalent checks.

## 10. Security & Safety

- No obvious unsafe code surface in reviewed files; this is a positive baseline.
- Primary safety risk is panic paths (macro internals + example `unwrap` usage), not memory unsafety.
- For untrusted input contexts:
  - Example API lacks duplicate ID handling and stronger lifecycle control, which can be abused for resource churn in long-running services.

## 11. Strategic Evaluation

### Top 3 non-idiomatic Rust issues that must be fixed

1. `unwrap/expect` in proc-macro internals where diagnostics should be structured `syn::Error`.
2. Ambiguous attribute combinations accepted by parser (`#[on]` and `#[on_timeout]` on same method).
3. Missing strict handler signature validation before codegen.

### Top 3 performance risks

1. Misleading benchmark throughput scenario (`Ping`-only fire loop).
2. Lock held across await in example request path.
3. Repeated reachability path checks in graph validation instead of single traversal.

### Top 3 blockers for open-source adoption

1. Broken quickstart command in example README.
2. Weak release workflow parity vs CI.
3. Missing contributor/security governance docs.

### What prevents production usage today

- Macro diagnostics and validation are not strict enough to guarantee predictable developer experience under malformed inputs.
- Example service code demonstrates patterns (mutex across await, duplicate-ID overwrite, unwrap boundaries) that are unsafe defaults if copied.
- Reliability signal is close but not complete due to benchmark flaws and a few flaky timing tests.

### Concrete next steps (ordered)

1. **Macro hardening (P0):** remove panic paths; return `syn::Error` everywhere; add signature/attribute conflict validation.
2. **Example correctness (P0):** fix duplicate ID semantics and avoid mutex guard across awaits in Axum example.
3. **Docs/CI alignment (P1):** fix run commands, align release checks with CI, document/verify MSRV.
4. **Reliability (P1):** convert sleep-based tests to paused-time/conditioned waits; add compile-fail tests for missing signature constraints.
5. **Performance credibility (P1):** correct benchmark scenarios to measure real state transitions.
6. **OSS maturity (P2):** add contributor and security policy docs.
