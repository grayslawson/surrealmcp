# PLAN: Restore Corrupted Files and Reapply Security Fixes

## Context
The `surrealmcp` project is in a broken state due to bad squash merges. `src/engine/mod.rs` is empty, and `src/tools/mod.rs` / `src/utils/mod.rs` need restoration and security hardening.

## Objectives
1.  **Recover Code**: Restore `src/tools/mod.rs`, `src/utils/mod.rs`, and `src/engine/mod.rs` from reliable git history (base: `deb6e0c`).
2.  **Security Hardening**: Reimplement and apply `is_safe_surrealql_snippet` to prevent SQL injection in tool parameters.
3.  **Authentication Cleanup**: Ensure `src/server/auth.rs` uses strict error handling rather than a dummy key fallback.
4.  **API Update**: Ensure `src/engine/mod.rs` correctly uses the SurrealDB 3.0 API.
5.  **Verification**: Confirm the project passes `cargo check`, `cargo test`, `cargo fmt`, and `cargo clippy`.

## Phase 1: Recovery and Foundation
- [ ] **Task 1.1**: Restore `src/tools/mod.rs` and `src/utils/mod.rs` from commit `deb6e0c`.
- [ ] **Task 1.2**: Check `src/engine/mod.rs` (it is currently 0 bytes) and restore it from commit `deb6e0c`.
- [ ] **Task 1.3**: **CRITICAL**: After restoring `src/tools/mod.rs`, locate and remove `pub fn new` from `SurrealService` (must use `with_config` only, per PR #5).
- [ ] **Task 1.4**: Verify basic compilation with `cargo check`.

## Phase 2: Security Implementation
- [ ] **Task 2.1**: Implement `is_safe_surrealql_snippet(input: &str) -> bool` in `src/utils/mod.rs`.
    - Must reject unquoted semicolons (`;`), `--`, and `/*`.
    - Must handle single and double quoted strings correctly.
- [ ] **Task 2.2**: Integrate the validator into `src/tools/mod.rs`.
    - Apply to: `where_clause`, `order_clause`, `group_clause`, `split_clause`, `limit_clause`, `start_clause` in `select`, `update`, `delete`, `upsert`.
    - Apply to: `table` parameter in `relate`.
    - Return `McpError::invalid_params` on failure.

## Phase 3: Verification and Refinement
- [ ] **Task 3.1**: Verify `src/server/auth.rs` has no `dummy-key` fallback (confirmed fixed on main).
- [ ] **Task 3.2**: Verify tests use `make_test_jwe_token(issuer: &str)`.
- [ ] **Task 3.3**: Ensure `src/engine/mod.rs` (now restored) uses `res.take(0)` and `into_mcp_result`.

## Phase 4: Final Verification and PR
- [ ] **Task 4.1**: Run `cargo fmt`.
- [ ] **Task 4.2**: Run `cargo clippy --all-targets --all-features -- -D warnings`.
- [ ] **Task 4.3**: Run `cargo test` and ensure all tests are green.
- [ ] **Task 4.4**: Submit PR as requested.

## Verification Criteria
- [ ] `grep "dummy-key" src/server/auth.rs` returns empty.
- [ ] `grep "eyJhbGciOiJkaXIi" src/server/auth.rs` returns empty.
- [ ] `grep "pub fn new" src/tools/mod.rs` (inside `SurrealService`) returns empty.
- [ ] Zero `cargo check` errors.
- [ ] Zero `clippy` warnings (as errors).
- [ ] All `cargo test` pass.
- [ ] PR titled: `fix: restore emptied files and reapply security fixes`.
