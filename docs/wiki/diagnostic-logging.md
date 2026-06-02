---
title: Diagnostic Logging
slug: diagnostic-logging
summary: Diagnostic logging uses an in-memory ring buffer with a toggle defaulting to off, ensuring zero overhead when disabled
tags:
  - capture
volatility: warm
confidence: medium
created: 2026-06-02
updated: 2026-06-02
verified: 2026-06-02
compiled-from: conversation
sources:
  - session:e1cfd663-230d-4f78-9078-0c9ed8b6a4bb
---

# Diagnostic Logging

## Architecture & Performance

Diagnostic logging uses an in-memory ring buffer with a toggle defaulting to off, ensuring zero overhead when disabled. The diagnostic log append function uses @autoclosure so that kernel-tick string interpolation is allocation-free when logging is disabled. <!-- [^e1cfd-3] -->

## Captured Events

Diagnostic logging captures kernel snapshot ticks, download events, audio commands, every kernel.dispatch call, store open failures, and app foreground/background transitions. <!-- [^e1cfd-4] -->

## App Lifecycle Integration

The app uses the modern scene-based lifecycle pattern, using @Environment(\.scenePhase) for actual foreground/background transitions and @UIApplicationDelegateAdaptor only for UIKit-owned responsibilities like background URL session reconnection and Siri shortcuts. App lifecycle logging is wired through the scenePhase switch in AppMain.swift, not AppDelegate lifecycle methods, because the app is scene-based and AppDelegate lifecycle methods never fire when scenes are active. <!-- [^e1cfd-5] -->

## User Interface

The diagnostic logs UI is located in Settings > Debug, featuring a toggle to enable logging and a log viewer with color-coded entries, copy-all, and clear actions. <!-- [^e1cfd-6] -->
