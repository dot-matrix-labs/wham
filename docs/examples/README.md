# Example Gallery

Each file in this directory shows the JavaScript host-side wiring for a complete form use-case. The Rust widget code (inside `crates/ui-wasm/src/`) is annotated in inline comments.

| File | Description |
|------|-------------|
| `login-form.js` | Email + password login, validation errors, loading spinner, server submission |
| `registration-form.js` | Multi-field registration, checkbox, select, inline field errors |
| `multi-step-checkout.js` | 3-step wizard (Shipping → Payment → Confirm), step validation, back navigation |
| `data-entry-table.js` | Repeating group rendered as a spreadsheet, scrollable body, auto-save debounce |

## How to use these examples

1. Pick the example closest to your use-case.
2. Read the Rust schema and `build()` annotations inside the file.
3. Copy the JS event-wiring and submission logic into your own `app.js`.
4. Implement the matching schema and widget calls in your Rust `FormApp`.
5. See `docs/api-reference.md` for the full widget and form API.
