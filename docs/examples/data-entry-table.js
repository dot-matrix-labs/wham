/**
 * data-entry-table — example Wasm app wiring for a spreadsheet-style data-entry form.
 *
 * Demonstrates:
 *   - A repeatable group rendered as a table (one row per entry)
 *   - Adding and removing rows dynamically
 *   - Scrollable container for large datasets
 *   - Auto-save on every change (debounced)
 *   - Inline validation per cell
 *
 * -----------------------------------------------------------------------
 * Rust schema (for reference):
 *
 *   FormSchema::new("inventory")
 *       .repeatable_group("rows", |g| g
 *           .field("sku",      FieldType::Text)
 *           .with_label("sku", "SKU")
 *           .required("sku")
 *           .field("name",     FieldType::Text)
 *           .with_label("name", "Product name")
 *           .required("name")
 *           .field("qty",      FieldType::Number)
 *           .with_label("qty", "Qty")
 *           .with_validation("qty", ValidationRule::NumberRange { min: Some(0.0), max: None })
 *           .field("price",    FieldType::Number)
 *           .with_label("price", "Price ($)")
 *       )
 *
 * Rust build() (for reference):
 *
 *   fn build(&mut self, ui: &mut Ui, form: &mut Form) {
 *       ui.label("Inventory");
 *
 *       // Table header
 *       ui.begin_row_with(&[2.0, 4.0, 1.0, 1.5]);
 *       for heading in &["SKU", "Product name", "Qty", "Price ($)"] {
 *           ui.label(heading);
 *       }
 *       ui.end_row();
 *
 *       // Scrollable body
 *       let rows_path = FormPath::root().push("rows");
 *       let row_count = row_count(form, &rows_path);
 *
 *       ui.begin_scroll("table-body", 400.0);
 *       for i in 0..row_count {
 *           ui.push_id(i);
 *           let base = rows_path.push(i.to_string());
 *
 *           ui.begin_row_with(&[2.0, 4.0, 1.0, 1.5]);
 *           ui.text_input_for(form, &base.push("sku"),   "SKU",  "");
 *           ui.text_input_for(form, &base.push("name"),  "Name", "");
 *           // Number inputs would need a custom widget; use text for this example:
 *           ui.text_input_for(form, &base.push("qty"),   "Qty",  "0");
 *           ui.text_input_for(form, &base.push("price"), "Price","0.00");
 *           ui.end_row();
 *
 *           ui.pop_id();
 *       }
 *       ui.end_scroll();
 *
 *       // Add row / Save buttons
 *       ui.begin_row();
 *       if ui.button("+ Add row") {
 *           form.add_repeat_group(&rows_path, row_fields());
 *       }
 *       if self.dirty {
 *           if form.pending().is_some() {
 *               ui.label("Saving…");
 *           } else if ui.button("Save") {
 *               // on_submit triggers auto-save
 *           }
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

  // Debounce auto-save: wait 1 s of inactivity after the last change.
  let saveTimer = null;

  function frame(timeMs) {
    const dpr = window.devicePixelRatio || 1;
    const w = canvas.clientWidth;
    const h = canvas.clientHeight;

    if (canvas.width !== Math.round(w * dpr) || canvas.height !== Math.round(h * dpr)) {
      canvas.width  = Math.round(w * dpr);
      canvas.height = Math.round(h * dpr);
    }

    const changed = app.frame_with_change_flag(collectEvents(), w, h, dpr, timeMs);
    const a11yJson = app.last_a11y_json();
    updateA11yMirror(JSON.parse(a11yJson));

    // Auto-save on change
    if (changed) {
      clearTimeout(saveTimer);
      saveTimer = setTimeout(() => {
        const sub = app.trigger_submit();
        if (sub) saveInventory(sub.id, sub.payload);
      }, 1000);
    }

    requestAnimationFrame(frame);
  }

  requestAnimationFrame(frame);
}

async function saveInventory(submissionId, payload) {
  try {
    const res = await fetch("/api/inventory", {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload),
    });

    if (res.ok) {
      app.apply_success(submissionId);
    } else {
      // On a save failure keep the form state (rollback = false) and show error.
      app.apply_error(submissionId, await res.text(), /* rollback= */ false);
    }
  } catch (err) {
    app.apply_error(submissionId, err.message, false);
  }
}

const pendingEvents = [];
["pointermove","pointerdown","pointerup","wheel"].forEach(type =>
  canvas.addEventListener(type, e => pendingEvents.push({ type, e }))
);
document.addEventListener("keydown", e => pendingEvents.push({ type: "keydown", e }));
document.addEventListener("paste",   e => pendingEvents.push({ type: "paste",   e }));

function collectEvents() {
  return JSON.stringify(pendingEvents.splice(0));
}

function updateA11yMirror(_tree) { /* see examples/web/app.js */ }

main();
