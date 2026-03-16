/**
 * login-form — example Wasm app wiring for a login form.
 *
 * This file shows how to host a wham Wasm module that implements a
 * login form (email + password fields, "Sign in" button, validation
 * errors, and a loading spinner during submission).
 *
 * The Rust side (crates/ui-wasm) implements FormApp::schema() and
 * FormApp::build() for this form. This JS file handles:
 *   1. Canvas setup and the requestAnimationFrame loop.
 *   2. Converting DOM events to wham InputEvent objects.
 *   3. Submitting the form payload to a real (or mock) API endpoint.
 *   4. Passing the accessibility tree to a screen-reader mirror.
 *
 * -----------------------------------------------------------------------
 * Rust schema (for reference):
 *
 *   FormSchema::new("login")
 *       .field("email", FieldType::Text)
 *       .with_label("email", "Email address")
 *       .required("email")
 *       .with_validation("email", ValidationRule::Email)
 *       .field("password", FieldType::Text)
 *       .with_label("password", "Password")
 *       .required("password")
 *
 * Rust build() (for reference):
 *
 *   fn build(&mut self, ui: &mut Ui, form: &mut Form) {
 *       ui.label("Sign in to your account");
 *
 *       let email_path = FormPath::root().push("email");
 *       ui.text_input_for(form, &email_path, "Email address", "you@example.com");
 *
 *       let pw_path = FormPath::root().push("password");
 *       ui.text_input_masked_for(form, &pw_path, "Password", "");
 *
 *       if let Some(err) = form.last_error() {
 *           ui.label_colored(err, Color::rgba(0.9, 0.2, 0.2, 1.0));
 *       }
 *
 *       if form.pending().is_some() {
 *           ui.label("Signing in…");
 *       } else if ui.button("Sign in") {
 *           // Handled in on_submit below
 *       }
 *   }
 * -----------------------------------------------------------------------
 */

import init, { WasmApp } from "./pkg/ui_wasm.js";

const canvas = document.getElementById("app");

async function main() {
  await init();

  // Create the Wasm app instance. The Rust side picks up the "login"
  // app variant based on a compile-time feature flag or dynamic dispatch.
  const app = new WasmApp(canvas);

  // --- rAF loop -----------------------------------------------------------
  function frame(timeMs) {
    const dpr = window.devicePixelRatio || 1;
    const w   = canvas.clientWidth;
    const h   = canvas.clientHeight;

    // Keep physical canvas size in sync with CSS size.
    if (canvas.width !== Math.round(w * dpr) || canvas.height !== Math.round(h * dpr)) {
      canvas.width  = Math.round(w * dpr);
      canvas.height = Math.round(h * dpr);
    }

    // Flush events collected since the last frame, then clear the queue.
    const events = collectEvents();

    // Run the Rust frame: layout, hit-test, render.
    const a11yJson = app.frame(events, w, h, dpr, timeMs);

    // Mirror the accessibility tree into a hidden DOM layer for screen readers.
    updateA11yMirror(JSON.parse(a11yJson));

    // If the form was submitted this frame, initiate the network request.
    const sub = app.take_pending_submission();
    if (sub) {
      submitLogin(sub.id, sub.payload);
    }

    requestAnimationFrame(frame);
  }

  requestAnimationFrame(frame);
}

// ---------------------------------------------------------------------------
// Network submission
// ---------------------------------------------------------------------------

async function submitLogin(submissionId, payload) {
  try {
    const res = await fetch("/api/login", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload),
    });

    if (res.ok) {
      // Tell the Wasm app that submission succeeded.
      app.apply_success(submissionId);
      // Navigate to the dashboard:
      window.location.href = "/dashboard";
    } else {
      const text = await res.text();
      // Tell the Wasm app that submission failed; roll back optimistic state.
      app.apply_error(submissionId, text, /* rollback= */ true);
    }
  } catch (err) {
    app.apply_error(submissionId, err.message, true);
  }
}

// ---------------------------------------------------------------------------
// Event collection (simplified — see examples/web/app.js for the full impl)
// ---------------------------------------------------------------------------

const pendingEvents = [];

canvas.addEventListener("pointermove",  e => pendingEvents.push(toPointerMove(e)));
canvas.addEventListener("pointerdown",  e => pendingEvents.push(toPointerDown(e)));
canvas.addEventListener("pointerup",    e => pendingEvents.push(toPointerUp(e)));
canvas.addEventListener("keydown",      e => pendingEvents.push(toKeyDown(e)));
canvas.addEventListener("keyup",        e => pendingEvents.push(toKeyUp(e)));
document.addEventListener("paste",      e => pendingEvents.push(toPaste(e)));

function collectEvents() {
  const batch = pendingEvents.splice(0);
  return JSON.stringify(batch);
}

// Stub converters — the real implementations live in examples/web/app.js.
function toPointerMove(e)  { /* ... */ return { type: "PointerMove", x: e.offsetX, y: e.offsetY }; }
function toPointerDown(e)  { /* ... */ return { type: "PointerDown", x: e.offsetX, y: e.offsetY, button: e.button }; }
function toPointerUp(e)    { /* ... */ return { type: "PointerUp",   x: e.offsetX, y: e.offsetY, button: e.button }; }
function toKeyDown(e)      { /* ... */ return { type: "KeyDown", code: e.code, key: e.key, shift: e.shiftKey, ctrl: e.ctrlKey, alt: e.altKey, meta: e.metaKey }; }
function toKeyUp(e)        { /* ... */ return { type: "KeyUp",   code: e.code, key: e.key, shift: e.shiftKey, ctrl: e.ctrlKey, alt: e.altKey, meta: e.metaKey }; }
function toPaste(e)        { /* ... */ return { type: "Paste", text: e.clipboardData?.getData("text") ?? "" }; }

// ---------------------------------------------------------------------------
// Accessibility mirror (simplified — see examples/web/app.js for full impl)
// ---------------------------------------------------------------------------

let mirrorRoot = document.createElement("div");
mirrorRoot.setAttribute("aria-live", "polite");
mirrorRoot.className = "sr-only";
document.body.appendChild(mirrorRoot);

function updateA11yMirror(tree) {
  // Walk the a11y tree and update hidden DOM elements so screen readers
  // can announce widget labels, values, and focus changes. In the full
  // implementation this creates <button>, <input>, <label> etc. elements
  // that are visually hidden but remain in the accessibility tree.
  //
  // See examples/web/app.js → AccessibilityMirror for the complete version.
}

main();
