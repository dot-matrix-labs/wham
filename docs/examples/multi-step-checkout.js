/**
 * multi-step-checkout — example Wasm app wiring for a 3-step checkout wizard.
 *
 * Demonstrates:
 *   - Wizard navigation (Shipping → Payment → Confirm)
 *   - Validating and saving each step before moving forward
 *   - Showing a progress indicator
 *   - Editing a previous step (navigate back without losing data)
 *   - Optimistic submission on the final step
 *
 * -----------------------------------------------------------------------
 * Rust state (for reference):
 *
 *   enum CheckoutStep { Shipping, Payment, Confirm }
 *
 *   struct CheckoutApp {
 *       step: CheckoutStep,
 *   }
 *
 * Rust schema (for reference):
 *
 *   FormSchema::new("checkout")
 *       // Shipping step
 *       .group("shipping", |g| g
 *           .field("name",    FieldType::Text).required("name")
 *           .field("address", FieldType::Text).required("address")
 *           .field("city",    FieldType::Text).required("city")
 *           .field("country", FieldType::Select { options: countries() }).required("country")
 *       )
 *       // Payment step
 *       .group("payment", |g| g
 *           .field("card_number", FieldType::Text).required("card_number")
 *           .field("expiry",      FieldType::Text).required("expiry")
 *           .field("cvv",         FieldType::Text).required("cvv")
 *       )
 *
 * Rust build() (for reference):
 *
 *   fn build(&mut self, ui: &mut Ui, form: &mut Form) {
 *       // Progress bar (3 steps)
 *       ui.label(match self.step {
 *           CheckoutStep::Shipping => "Step 1 of 3 — Shipping",
 *           CheckoutStep::Payment  => "Step 2 of 3 — Payment",
 *           CheckoutStep::Confirm  => "Step 3 of 3 — Confirm",
 *       });
 *
 *       match self.step {
 *           CheckoutStep::Shipping => self.build_shipping(ui, form),
 *           CheckoutStep::Payment  => self.build_payment(ui, form),
 *           CheckoutStep::Confirm  => self.build_confirm(ui, form),
 *       }
 *   }
 *
 *   fn build_shipping(&mut self, ui: &mut Ui, form: &mut Form) {
 *       let base = FormPath::root().push("shipping");
 *       ui.text_input_for(form, &base.push("name"),    "Full name",  "");
 *       ui.text_input_for(form, &base.push("address"), "Address",    "");
 *       ui.text_input_for(form, &base.push("city"),    "City",       "");
 *
 *       if ui.button("Continue to Payment") {
 *           if form.validate().is_ok() {
 *               self.step = CheckoutStep::Payment;
 *           }
 *       }
 *   }
 *
 *   fn build_payment(&mut self, ui: &mut Ui, form: &mut Form) {
 *       let base = FormPath::root().push("payment");
 *       ui.text_input_for(form, &base.push("card_number"), "Card number", "1234 5678 9012 3456");
 *       ui.text_input_for(form, &base.push("expiry"),      "Expiry",      "MM/YY");
 *       ui.text_input_masked_for(form, &base.push("cvv"), "CVV", "");
 *
 *       ui.begin_row();
 *       if ui.button("Back") { self.step = CheckoutStep::Shipping; }
 *       if ui.button("Review order") {
 *           if form.validate().is_ok() { self.step = CheckoutStep::Confirm; }
 *       }
 *       ui.end_row();
 *   }
 *
 *   fn build_confirm(&mut self, ui: &mut Ui, form: &mut Form) {
 *       ui.label("Review your order before placing it.");
 *       // … display summary …
 *
 *       ui.begin_row();
 *       if ui.button("Edit") { self.step = CheckoutStep::Payment; }
 *       if form.pending().is_some() {
 *           ui.label("Placing order…");
 *       } else if ui.button("Place order") {
 *           // on_submit calls form.start_submit
 *       }
 *       ui.end_row();
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
    if (sub) placeOrder(sub.id, sub.payload);

    requestAnimationFrame(frame);
  }

  requestAnimationFrame(frame);
}

async function placeOrder(submissionId, payload) {
  try {
    const res = await fetch("/api/orders", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload),
    });

    if (res.ok) {
      const { orderId } = await res.json();
      app.apply_success(submissionId);
      window.location.href = `/order-confirmation?id=${orderId}`;
    } else {
      app.apply_error(submissionId, await res.text(), true);
    }
  } catch (err) {
    app.apply_error(submissionId, err.message, true);
  }
}

const pendingEvents = [];
["pointermove","pointerdown","pointerup","wheel"].forEach(type =>
  canvas.addEventListener(type, e => pendingEvents.push({ type, e }))
);
document.addEventListener("keydown", e => pendingEvents.push({ type: "keydown", e }));

function collectEvents() {
  return JSON.stringify(pendingEvents.splice(0));
}

function updateA11yMirror(_tree) { /* see examples/web/app.js */ }

main();
