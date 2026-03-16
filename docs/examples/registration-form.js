/**
 * registration-form — example Wasm app wiring for a user-registration form.
 *
 * Demonstrates:
 *   - Multiple text fields (first name, last name, email, password, confirm-password)
 *   - A checkbox ("I agree to the terms")
 *   - A select widget ("Country")
 *   - Cross-field validation (passwords must match — via custom server rule)
 *   - Inline error display next to each field
 *   - Disabling the submit button while pending
 *
 * -----------------------------------------------------------------------
 * Rust schema (for reference):
 *
 *   FormSchema::new("registration")
 *       .field("given_name",  FieldType::Text)
 *       .with_label("given_name",  "First name")
 *       .required("given_name")
 *       .field("family_name", FieldType::Text)
 *       .with_label("family_name", "Last name")
 *       .required("family_name")
 *       .field("email", FieldType::Text)
 *       .with_label("email", "Email address")
 *       .required("email")
 *       .with_validation("email", ValidationRule::Email)
 *       .field("password", FieldType::Text)
 *       .with_label("password", "Password")
 *       .required("password")
 *       .field("country", FieldType::Select {
 *           options: vec!["AU", "CA", "GB", "US", ...].iter().map(|s| s.to_string()).collect()
 *       })
 *       .with_label("country", "Country")
 *       .required("country")
 *       .field("terms", FieldType::Checkbox)
 *       .with_label("terms", "I accept the terms and conditions")
 *       .required("terms")
 *
 * Rust build() (for reference):
 *
 *   fn build(&mut self, ui: &mut Ui, form: &mut Form) {
 *       ui.label("Create an account");
 *
 *       ui.begin_row_with(&[1.0, 1.0]);
 *       ui.text_input_for(form, &path("given_name"),  "First name", "");
 *       ui.text_input_for(form, &path("family_name"), "Last name",  "");
 *       ui.end_row();
 *
 *       ui.text_input_for(form, &path("email"),    "Email",    "you@example.com");
 *       ui.text_input_masked_for(form, &path("password"), "Password", "");
 *
 *       let country_options = self.countries.clone();
 *       let mut country_val = current_selection(form, &path("country"));
 *       if ui.select("Country", &country_options, &mut country_val) {
 *           form.set_value(&path("country"), FieldValue::Selection(country_val));
 *       }
 *
 *       let mut terms = current_bool(form, &path("terms"));
 *       if ui.checkbox("I accept the terms and conditions", &mut terms) {
 *           form.set_value(&path("terms"), FieldValue::Bool(terms));
 *       }
 *
 *       // Show field errors inline
 *       for field_id in &["given_name", "family_name", "email", "password", "country", "terms"] {
 *           if let Some(fs) = form.state().get_field(&path(field_id)) {
 *               for e in &fs.errors {
 *                   ui.label_colored(e, ERROR_COLOR);
 *               }
 *           }
 *       }
 *
 *       if form.pending().is_some() {
 *           ui.label("Creating account…");
 *       } else {
 *           if ui.button("Create account") { /* submit handled by on_submit */ }
 *       }
 *   }
 * -----------------------------------------------------------------------
 */

import init, { WasmApp } from "./pkg/ui_wasm.js";

const canvas = document.getElementById("app");

async function main() {
  await init();
  const app = new WasmApp(canvas);

  function frame(timeMs) {
    const dpr = window.devicePixelRatio || 1;
    const w = canvas.clientWidth;
    const h = canvas.clientHeight;

    if (canvas.width !== Math.round(w * dpr) || canvas.height !== Math.round(h * dpr)) {
      canvas.width  = Math.round(w * dpr);
      canvas.height = Math.round(h * dpr);
    }

    const a11yJson = app.frame(collectEvents(), w, h, dpr, timeMs);
    updateA11yMirror(JSON.parse(a11yJson));

    const sub = app.take_pending_submission();
    if (sub) registerUser(sub.id, sub.payload);

    requestAnimationFrame(frame);
  }

  requestAnimationFrame(frame);
}

async function registerUser(submissionId, payload) {
  try {
    const res = await fetch("/api/register", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload),
    });

    if (res.ok) {
      app.apply_success(submissionId);
      window.location.href = "/welcome";
    } else {
      // The server may return field-level errors:
      const body = await res.json().catch(() => ({ error: "Unknown error" }));
      app.apply_error(submissionId, body.error ?? JSON.stringify(body), false);
    }
  } catch (err) {
    app.apply_error(submissionId, err.message, true);
  }
}

// --- Event wiring (abbreviated — see login-form.js for detail) -----------
const pendingEvents = [];
["pointermove","pointerdown","pointerup"].forEach(type =>
  canvas.addEventListener(type, e => pendingEvents.push({ type, e }))
);
document.addEventListener("keydown", e => pendingEvents.push({ type: "keydown", e }));
document.addEventListener("paste",   e => pendingEvents.push({ type: "paste",   e }));

function collectEvents() {
  return JSON.stringify(pendingEvents.splice(0));
}

function updateA11yMirror(_tree) { /* see examples/web/app.js */ }

main();
