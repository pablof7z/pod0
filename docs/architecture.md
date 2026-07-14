# Architecture

## Overview

The template follows a **single-store, observable** pattern derived from win-the-day-app's `AppStateStore`. There is no MVVM layer, no TCA effects system — just:

1. `AppState` — a `Codable` struct (the data)
2. `AppStateStore` — an `@Observable` class wrapping `AppState` (the mutations)
3. SwiftUI views that read from the store via `@Environment`

## Data flow

```
User action
    → View calls AppStateStore method
        → Store mutates state
            → didSet fires → Persistence.save()
                → UserDefaults updated
                    → @Observable propagates to dependent views
                        → SwiftUI re-renders
```

Agent actions follow the same path: `AgentTools.dispatch()` calls the same `AppStateStore` methods as the UI.

## Module boundaries

```
AppState (Codable, Sendable)
  ↓ owns
  [Item], [Note], [Friend], [AgentMemory], Settings

AppStateStore (@Observable, @MainActor)
  ↓ wraps
  AppState
  ↓ calls
  Persistence.save() on every mutation

AgentSession (@Observable, @MainActor)
  ↓ reads
  AppStateStore.state (for system prompt)
  ↓ writes via
  AgentTools.dispatch() → AppStateStore methods

FeedbackWorkflow (@Observable, @MainActor)
  ↓ owned by
  RootView (top-level @State)
  ↓ drives
  FeedbackView, ScreenshotAnnotationView sheets
```

## Key decisions

### Why JSON blob over SwiftData?

JSON blob to UserDefaults (app group) is:
- Zero schema migration complexity for small data
- Trivially shareable with widgets/extensions (just read the same key)
- Easy to backup, inspect, and debug
- Fully compatible with CloudKit KVS sync

Secrets do not go in this blob. OpenRouter keys from BYOK or manual entry are stored in Keychain via `OpenRouterCredentialStore`; `Settings` stores only non-secret connection metadata.

For large datasets (thousands of items, time-series data), use SwiftData like cut-tracker does. The `Persistence` module is the only change needed.

### Why @Observable over ObservableObject?

Swift 5.9+ `@Observable` avoids the `objectWillChange` publisher firing on every property change. Only views that actually read a property re-render when that property changes. This is strictly better than `@Published` on a monolithic `@StateObject`.

### Why not TCA?

TCA (The Composable Architecture) is excellent for large teams and complex side effects. This template favors simplicity: direct method calls, no reducer boilerplate, no effect publishers. Add TCA if your project needs it.

### Swift 6 strict concurrency

All models are `Sendable`. `AppStateStore` and `AgentSession` are `@MainActor`. `AgentTools.dispatch` is `@MainActor`. This satisfies Swift 6's strict concurrency without data races.

## Liquid Glass design system (iOS 26)

The template targets iOS 26 and uses Apple's native Liquid Glass material throughout:

- **`GlassSurface.swift`** — `.glassSurface(cornerRadius:)` modifier backed by `.glassEffect()`. Tinted variant for status banners.
- **`HomeView`** — Glass FAB row (Add + Ask Agent) at the bottom, glass agent-status banner with tint keyed to session state.
- **`FeedbackView`** — TextEditor rendered on a glass surface; screenshot action buttons use `.buttonStyle(.glass)`.
- **`FriendDetailView`** — Profile header card uses `.glassSurface()` pinned above the List.
- Toolbar and tab bar glass are handled automatically by iOS 26; no additional code required.

Key patterns:
- Always wrap sibling glass elements in `GlassEffectContainer(spacing:)` for performance and morphing.
- Use `glassEffectID(_:in:)` + `@Namespace` for smooth morphing transitions.
- Use `.interactive()` only on controls that respond to touch (buttons, toggles).
- Avoid stacking too many nested glass layers — reserve for key chrome and interactive elements.

## Extension points

| What | Where | How |
|------|-------|-----|
| New data type | `Domain/Models.swift` → `AppState` | Add `var things: [Thing] = []` to AppState |
| New mutation | `State/AppStateStore.swift` | Add a method that mutates `state.X` |
| New agent tool | `Agent/AgentTools.swift` | Add to `schema` and `dispatch` |
| New tab | `App/RootView.swift` | Add to `RootTab`, add `NavigationStack` in switch |
| New friend identifier type | `Domain/Models.swift` `Friend.identifier` | Change to an enum or typed ID |
| iCloud sync | `State/Persistence.swift` | Add NSUbiquitousKeyValueStore alongside UserDefaults |
| SwiftData | Replace `Persistence.swift` | Use `ModelContainer` + `@Model` classes |
| Watch extension | Add target to `Project.swift` | Communicate via `WCSession` + App Group |
| Widget | Add target to `Project.swift` | Share state via App Group UserDefaults |
| Glass tint for new status | `GlassSurface.swift` extension | Use `.glassSurface(cornerRadius:tint:)` overload |
