# Agent Guidelines

## Typography

**No serif fonts, ever.** Do not use `.serif` font design, `NewYork`, `NewYork-SemiboldItalic`, or any other serif typeface anywhere in the app. All text must use SF (system font). For italic style, use `UIFont.italicSystemFont` or `.italic()` modifier — never a serif variant.

## File Length Limits

- **Soft limit: 300 lines** — prefer splitting into smaller files when approaching this threshold.
- **Hard limit: 500 lines** — files must not exceed 500 lines. Refactor before adding more code.
