# Architecture and ownership

These rules apply to all changes, regardless of the editor or coding tool.

## Dependency boundaries

- UI components render state and submit intent through the application service interface. They do not parse runtime configuration, call the kernel directly, or apply OS network changes. Keep typed Tauri calls in the client service boundary; keep native commands as adapters to their owning services.
- The Rust host owns persistence, permissions, native side effects and runtime coordination. Keep platform-independent decisions out of shell, WebView and platform adapters. Do not create a second path for an operation already owned by a service.
- Plugins use scoped host capabilities. Visibility, a saved preference or a claimed plugin identity is not authorization. Do not bypass the existing capability owner to simplify a feature.

## Configuration and runtime

| State | Authority | Rule |
| --- | --- | --- |
| Local UI preferences | The user on this client | Subscription and imported network data cannot set layout, mode, navigation or tool visibility. |
| Source material | Subscription, local file or authorized plugin | Treat it as network input. Validate it at the existing boundary. Keep source material separate from local edits. |
| Local network edits | Client-wide or profile-scoped, as defined by the setting | Store changes in the appropriate scope. Do not turn a profile edit into a global override. |
| Effective runtime state | The host and the relevant Zero instance | Saving or dispatching a candidate does not prove application. Only report confirmed effects after authoritative acknowledgment. Distinguish failure from an unknown outcome. |

Lite and Pro use the same application commands and runtime state. Changing their presentation must not restart the proxy or modify network configuration. Preserve already working settings after a rejected candidate; for an uncertain result, reconcile instead of declaring success or retrying a side effect blindly.

## Changing a boundary

Change the owning API, type or schema before adding a workaround at its call sites. For a new rule, supply a counterexample and put the check at the smallest boundary that can reject it. Do not leave an exception that other callers can copy without an owner and a removal condition.
