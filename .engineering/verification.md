# Verification and review

## Choose evidence that matches the change

| Change | Evidence |
| --- | --- |
| Bug fix | Add a regression at the owner boundary; show that it distinguishes the broken behavior from the expected behavior. |
| Configuration or service contract | Test accepted input and a rejected or conflicting input. Verify what was saved and what actually took effect separately. |
| Desktop interaction | Exercise the real user entry point; cover the normal path and relevant failure or recovery. Attach a screenshot or recording for visual changes, but do not use it as proof of OS effects. |
| Native network or platform behavior | Verify on the affected platform in an isolated environment. Report which combinations were exercised; browser fixtures and package builds are not installed network tests. |
| CI, fixtures or acceptance rules | Explain the rule change and preserve a counterexample that the changed check still catches. Require human review before merging changes to the gate. |

## Reuse the existing checks

- Use the narrowest existing regression first. For cross-layer work, run `pnpm verify` when the environment supports it. For browser interaction, also use `pnpm test:ui-browser`; it is not part of `pnpm verify`.
- For Rust subscription changes, use `cargo test --manifest-path src-tauri/Cargo.toml --locked --lib subscription_`. The existing Windows CI job also runs the release-mode subscription suite. Do not treat frontend tests as a substitute for the Rust suite.
- For shared contracts, keep the existing desktop and mobile regression checks. Mobile build, device or VPN qualification is a separate claim.
- Reserve host installation and privileged network checks for the intended platform. Do not run them against a live user setup by default.

## Review and delivery

The PR must state the behavioral change, where the constraint is enforced, the checks that passed or failed, and any checks not run. Report simulator, mock, real-kernel and installed-device evidence separately. A green build alone does not establish that the feature works.

Do not skip checks, relax assertions, broaden retries or suppress errors just to pass CI. If a contract genuinely changes, change the owner and its tests together, and explain the effect on callers and persisted data.
