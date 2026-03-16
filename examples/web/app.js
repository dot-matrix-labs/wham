import init, { WasmApp } from "./pkg/ui_wasm.js";

const canvas = document.getElementById("app");
const dpr = window.devicePixelRatio || 1;

// ---------------------------------------------------------------------------
// Frame budget monitoring
//
// Track frame times using a rolling window. Warn if 5+ consecutive frames
// exceed the 12 ms budget. Store the last frame time on a module-level
// variable so other code (and debug overlay) can read it.
// ---------------------------------------------------------------------------

/** Rolling window of recent frame times in ms (capped at FRAME_WINDOW_SIZE). */
const FRAME_TIMES = [];
const FRAME_WINDOW_SIZE = 60;
const FRAME_BUDGET_MS = 12;
const OVERRUN_WARN_THRESHOLD = 5;

/** Last measured frame time in ms. Available to WASM runtime if needed. */
let lastFrameTimeMs = 0;

/** Count of consecutive frames that exceeded the budget. */
let consecutiveOverruns = 0;

/**
 * Record a frame time sample, updating the rolling window and consecutive
 * overrun counter. Emits a console.warn when sustained overruns are detected.
 *
 * @param {number} frameMs  Frame duration in milliseconds.
 */
function recordFrameTime(frameMs) {
  lastFrameTimeMs = frameMs;
  FRAME_TIMES.push(frameMs);
  if (FRAME_TIMES.length > FRAME_WINDOW_SIZE) {
    FRAME_TIMES.shift();
  }

  if (frameMs > FRAME_BUDGET_MS) {
    consecutiveOverruns += 1;
    if (consecutiveOverruns === OVERRUN_WARN_THRESHOLD) {
      const avg = (FRAME_TIMES.slice(-OVERRUN_WARN_THRESHOLD)
        .reduce((s, t) => s + t, 0) / OVERRUN_WARN_THRESHOLD).toFixed(1);
      console.warn(
        `[wham] Frame budget overrun: ${consecutiveOverruns} consecutive frames ` +
        `exceeded ${FRAME_BUDGET_MS} ms (avg ${avg} ms over last ${OVERRUN_WARN_THRESHOLD} frames)`
      );
    }
  } else {
    consecutiveOverruns = 0;
  }
}

// ---------------------------------------------------------------------------
// Debug overlay
//
// When window.__WHAM_DEBUG is true before init(), a small DOM overlay is
// rendered in the top-right corner showing FPS and frame time.
// ---------------------------------------------------------------------------

/**
 * Create a DOM-based frame stats overlay and return an update function.
 * The overlay is styled to be non-interactive and visually minimal.
 *
 * @returns {{ update: (frameMs: number) => void, el: HTMLElement }}
 */
function createFrameStatsOverlay() {
  const el = document.createElement("div");
  Object.assign(el.style, {
    position: "fixed",
    top: "8px",
    right: "8px",
    padding: "4px 8px",
    background: "rgba(0,0,0,0.65)",
    color: "#0f0",
    fontFamily: "monospace",
    fontSize: "11px",
    lineHeight: "1.4",
    borderRadius: "4px",
    pointerEvents: "none",
    zIndex: "9999",
    userSelect: "none",
    whiteSpace: "pre",
  });
  el.setAttribute("aria-hidden", "true");
  document.body.appendChild(el);

  function update(frameMs) {
    const fps = frameMs > 0 ? (1000 / frameMs).toFixed(1) : "---";
    const over = frameMs > FRAME_BUDGET_MS ? " !" : "  ";
    el.textContent = `FPS  ${fps.padStart(6)}\nms  ${frameMs.toFixed(2).padStart(6)}${over}`;
  }

  return { update, el };
}

// ---------------------------------------------------------------------------
// AccessibilityMirror — hidden DOM tree that mirrors canvas widgets for
// screen readers.  Elements are visually hidden (sr-only pattern) but remain
// in the accessibility tree so that NVDA, VoiceOver, TalkBack, etc. can
// navigate and interact with the GPU-rendered UI.
// ---------------------------------------------------------------------------
class AccessibilityMirror {
  /**
   * @param {HTMLCanvasElement} canvas  The rendering canvas.
   * @param {WasmApp}           app     Reference to the Wasm application.
   * @param {number}            dpr     Device pixel ratio.
   */
  constructor(canvas, app, dpr) {
    this._app = app;
    this._dpr = dpr;
    this._nodes = new Map(); // id -> DOM element
    this._suppressFocusSync = false;

    // Container — positioned over the canvas, transparent to pointer events,
    // but visible to the accessibility tree.
    this._container = document.createElement("div");
    this._container.setAttribute("role", "application");
    this._container.setAttribute("aria-label", "GPU Forms UI");
    this._container.setAttribute("aria-hidden", "false");
    Object.assign(this._container.style, {
      position: "absolute",
      top: "0",
      left: "0",
      width: "100%",
      height: "100%",
      pointerEvents: "none",
      overflow: "hidden",
      // Do not affect document layout.
      contain: "strict",
    });
    // Insert immediately after the canvas so the tab order is natural.
    canvas.parentNode.insertBefore(this._container, canvas.nextSibling);

    // Live region for status announcements (form errors, submission results).
    this._liveRegion = document.createElement("div");
    this._liveRegion.setAttribute("aria-live", "polite");
    this._liveRegion.setAttribute("aria-atomic", "true");
    Object.assign(this._liveRegion.style, {
      position: "absolute",
      width: "1px",
      height: "1px",
      overflow: "hidden",
      clip: "rect(0 0 0 0)",
      clipPath: "inset(50%)",
      whiteSpace: "nowrap",
    });
    this._container.appendChild(this._liveRegion);
  }

  /**
   * Update the mirror to match the latest accessibility tree from Wasm.
   *
   * @param {object} a11yTree  Deserialized A11yTree (root: A11yNode).
   */
  update(a11yTree) {
    if (!a11yTree || !a11yTree.root) return;
    const children = a11yTree.root.children || [];
    const seenIds = new Set();

    for (const node of children) {
      seenIds.add(this._idKey(node.id));
      this._upsertNode(node);
    }

    // Remove stale nodes that are no longer in the tree.
    for (const [key, el] of this._nodes) {
      if (!seenIds.has(key)) {
        el.remove();
        this._nodes.delete(key);
      }
    }
  }

  /**
   * Sync DOM focus to match the Wasm-side focused widget id.
   * @param {number|null} focusedId  The id of the focused widget (BigInt or Number), or null.
   */
  syncFocus(focusedId) {
    if (focusedId == null) return;
    const key = this._idKey(focusedId);
    const el = this._nodes.get(key);
    if (el && document.activeElement !== el) {
      this._suppressFocusSync = true;
      el.focus({ preventScroll: true });
      this._suppressFocusSync = false;
    }
  }

  /**
   * Announce a message to screen readers via the live region.
   * @param {string} message
   */
  announce(message) {
    // Clear and re-set so the same message can be announced twice in a row.
    this._liveRegion.textContent = "";
    // Use a microtask break so the browser registers the empty → filled change.
    requestAnimationFrame(() => {
      this._liveRegion.textContent = message;
    });
  }

  // -- private helpers ------------------------------------------------------

  _idKey(id) {
    // A11y ids may arrive as BigInt from serde_wasm_bindgen; normalise to string.
    return String(id);
  }

  _upsertNode(node) {
    const key = this._idKey(node.id);
    let el = this._nodes.get(key);
    const role = node.role;

    if (!el) {
      el = this._createElement(role);
      this._attachFocusHandler(el, node.id);
      this._container.appendChild(el);
      this._nodes.set(key, el);
    }

    // Update content & ARIA attributes.
    this._updateElement(el, node);
    this._positionElement(el, node.bounds);
  }

  _createElement(role) {
    let el;
    switch (role) {
      case "TextBox":
        el = document.createElement("input");
        el.type = "text";
        el.setAttribute("role", "textbox");
        break;
      case "Button":
        el = document.createElement("button");
        break;
      case "CheckBox":
        el = document.createElement("input");
        el.type = "checkbox";
        el.setAttribute("role", "checkbox");
        break;
      case "RadioButton":
        el = document.createElement("input");
        el.type = "radio";
        el.setAttribute("role", "radio");
        break;
      case "ComboBox":
        el = document.createElement("select");
        el.setAttribute("role", "combobox");
        break;
      case "Label":
        el = document.createElement("span");
        el.setAttribute("role", "note");
        break;
      case "Group":
        el = document.createElement("fieldset");
        el.setAttribute("role", "group");
        break;
      default:
        el = document.createElement("span");
        break;
    }

    // sr-only styling: visually hidden but accessible.
    Object.assign(el.style, {
      position: "absolute",
      overflow: "hidden",
      clip: "rect(0 0 0 0)",
      clipPath: "inset(50%)",
      width: "1px",
      height: "1px",
      whiteSpace: "nowrap",
      border: "0",
      padding: "0",
      margin: "-1px",
      pointerEvents: "auto", // allow screen reader interaction
    });
    el.tabIndex = 0;

    return el;
  }

  _updateElement(el, node) {
    el.setAttribute("aria-label", node.name || "");
    el.setAttribute("data-wham-id", this._idKey(node.id));

    if (node.value != null) {
      if (el.tagName === "INPUT" && el.type === "text") {
        el.value = node.value;
      } else if (el.tagName === "SELECT") {
        // nothing — options would need to be populated separately
      } else {
        el.setAttribute("aria-valuenow", node.value);
      }
    }

    const st = node.state || {};
    if (st.disabled) {
      el.setAttribute("aria-disabled", "true");
      el.disabled = true;
    } else {
      el.removeAttribute("aria-disabled");
      el.disabled = false;
    }

    if (st.invalid) {
      el.setAttribute("aria-invalid", "true");
    } else {
      el.removeAttribute("aria-invalid");
    }

    if (st.required) {
      el.setAttribute("aria-required", "true");
    } else {
      el.removeAttribute("aria-required");
    }

    if (node.role === "CheckBox") {
      el.checked = !!st.selected;
      el.setAttribute("aria-checked", st.selected ? "true" : "false");
    }

    if (node.role === "RadioButton") {
      el.checked = !!st.selected;
      el.setAttribute("aria-checked", st.selected ? "true" : "false");
    }

    if (node.role === "ComboBox") {
      el.setAttribute("aria-expanded", st.expanded ? "true" : "false");
    }
  }

  _positionElement(el, bounds) {
    if (!bounds) return;
    // bounds are in canvas (physical) pixels — convert to CSS pixels.
    const x = bounds.x / this._dpr;
    const y = bounds.y / this._dpr;
    const w = bounds.w / this._dpr;
    const h = bounds.h / this._dpr;
    el.style.left = `${x}px`;
    el.style.top = `${y}px`;
    // Override sr-only 1px sizing with actual widget bounds so screen reader
    // spatial navigation knows where elements are, but keep clip to hide them.
    el.style.width = `${Math.max(w, 1)}px`;
    el.style.height = `${Math.max(h, 1)}px`;
  }

  _attachFocusHandler(el, nodeId) {
    el.addEventListener("focus", () => {
      if (this._suppressFocusSync) return;
      // Coerce BigInt to Number for the Wasm FFI boundary.
      const id = typeof nodeId === "bigint" ? Number(nodeId) : nodeId;
      this._app.set_focus(id);
    });
  }
}

function resize() {
  canvas.width = window.innerWidth * dpr;
  canvas.height = window.innerHeight * dpr;
  canvas.style.width = `${window.innerWidth}px`;
  canvas.style.height = `${window.innerHeight}px`;
}

/**
 * Read CSS env(safe-area-inset-*) values via a hidden probe element.
 *
 * Browsers expose hardware safe-area insets (notch, home indicator, rounded
 * corners) only through CSS `env()`. We create a tiny off-screen element with
 * inline `padding` set to each env value and measure the computed padding via
 * `getComputedStyle`. The element is reused across calls.
 *
 * @returns {{ top: number, right: number, bottom: number, left: number }}
 */
let _safeAreaProbe = null;
function readSafeAreaInsets() {
  if (!_safeAreaProbe) {
    _safeAreaProbe = document.createElement("div");
    Object.assign(_safeAreaProbe.style, {
      position: "fixed",
      top: "0",
      left: "0",
      width: "0",
      height: "0",
      // Each padding edge reads one env() value. On browsers / devices that
      // do not support these env() variables the padding falls back to 0px.
      paddingTop: "env(safe-area-inset-top, 0px)",
      paddingRight: "env(safe-area-inset-right, 0px)",
      paddingBottom: "env(safe-area-inset-bottom, 0px)",
      paddingLeft: "env(safe-area-inset-left, 0px)",
      pointerEvents: "none",
      visibility: "hidden",
    });
    document.body.appendChild(_safeAreaProbe);
  }
  const style = getComputedStyle(_safeAreaProbe);
  return {
    top: parseFloat(style.paddingTop) || 0,
    right: parseFloat(style.paddingRight) || 0,
    bottom: parseFloat(style.paddingBottom) || 0,
    left: parseFloat(style.paddingLeft) || 0,
  };
}

/**
 * Read the current safe area insets and forward them to the WASM runtime.
 *
 * @param {WasmApp} app
 */
function updateSafeAreaInsets(app) {
  const insets = readSafeAreaInsets();
  app.set_safe_area_insets(insets.top, insets.right, insets.bottom, insets.left);
}

/**
 * Create a hidden textarea overlaying the canvas for mobile keyboard input.
 *
 * iOS Safari scrolls the viewport to bring focused elements into view, even
 * when they are positioned offscreen (e.g. `left:-9999px`). To prevent this
 * we keep the textarea at `opacity:0` directly over the focused canvas widget
 * instead of hiding it offscreen.
 */
function createHiddenTextarea() {
  const ta = document.createElement("textarea");
  ta.setAttribute("autocapitalize", "off");
  ta.setAttribute("autocomplete", "off");
  ta.setAttribute("autocorrect", "off");
  ta.setAttribute("spellcheck", "false");
  ta.setAttribute("aria-hidden", "true");
  ta.setAttribute("tabindex", "-1");
  Object.assign(ta.style, {
    position: "absolute",
    top: "0px",
    left: "0px",
    width: "1px",
    height: "1px",
    opacity: "0",
    pointerEvents: "none",
    zIndex: "-1",
    padding: "0",
    border: "none",
    outline: "none",
    resize: "none",
    overflow: "hidden",
    // Prevent iOS zoom on focus
    fontSize: "16px",
  });
  document.body.appendChild(ta);
  return ta;
}

/**
 * Reposition the hidden textarea to overlay the focused widget so that iOS
 * Safari does not scroll the viewport when the textarea receives focus.
 *
 * Coordinates from the WASM side are in canvas (physical) pixels; we convert
 * to CSS pixels by dividing by `dpr`.
 */
function repositionTextarea(ta, rectArray, dpr) {
  if (!rectArray) {
    // No focused widget — park the textarea at origin so it stays harmless.
    ta.style.top = "0px";
    ta.style.left = "0px";
    ta.style.width = "1px";
    ta.style.height = "1px";
    return;
  }
  const x = rectArray[0] / dpr;
  const y = rectArray[1] / dpr;
  const w = rectArray[2] / dpr;
  const h = rectArray[3] / dpr;
  ta.style.left = `${x}px`;
  ta.style.top = `${y}px`;
  ta.style.width = `${Math.max(w, 1)}px`;
  ta.style.height = `${Math.max(h, 1)}px`;
}

// ---------------------------------------------------------------------------
// Theme management — prefers-color-scheme + animated transitions
// ---------------------------------------------------------------------------

/**
 * Convert a CSS hex color string ("#rrggbb" or "#rrggbbaa") to an [r,g,b,a]
 * array with channel values in [0, 1].  Returns null on parse failure.
 * @param {string} hex
 * @returns {[number,number,number,number]|null}
 */
function hexToRgba(hex) {
  const m = /^#([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})?$/i.exec(hex.trim());
  if (!m) return null;
  return [
    parseInt(m[1], 16) / 255,
    parseInt(m[2], 16) / 255,
    parseInt(m[3], 16) / 255,
    m[4] ? parseInt(m[4], 16) / 255 : 1.0,
  ];
}

/**
 * ThemeController drives dark-mode detection and animated theme transitions.
 *
 * Instantiate once after the WasmApp is created.  Call `tick(now)` every
 * animation frame so in-progress transitions advance smoothly.
 */
class ThemeController {
  /**
   * @param {WasmApp} app
   */
  constructor(app) {
    this._app = app;
    this._customOverrides = null;

    // System media queries.
    this._darkMq = window.matchMedia("(prefers-color-scheme: dark)");
    this._reducedMotionMq = window.matchMedia("(prefers-reduced-motion: reduce)");

    this._targetDark = this._darkMq.matches;

    // Transition state.
    this._TRANSITION_MS = 300;
    this._transitionStart = null;
    this._transitionFrom = null;  // "light" | "dark"
    this._transitionTo = null;    // "light" | "dark"
    this._inTransition = false;

    // Apply the initial theme immediately (no animation on first paint).
    this._applyInstant(this._targetDark);

    // React to OS preference changes.
    this._darkMq.addEventListener("change", (e) => {
      this._startTransition(e.matches);
    });
  }

  /**
   * Advance any in-progress theme transition.  Call once per rAF frame.
   * @param {number} now  Timestamp in ms (e.g. from requestAnimationFrame).
   */
  tick(now) {
    if (!this._inTransition) return;

    const elapsed = now - this._transitionStart;
    const t = Math.min(elapsed / this._TRANSITION_MS, 1.0);

    const fromProgress = this._transitionFrom === "dark" ? 1.0 : 0.0;
    const toProgress   = this._transitionTo   === "dark" ? 1.0 : 0.0;
    const progress = fromProgress + (toProgress - fromProgress) * t;

    this._applyProgress(progress);

    if (t >= 1.0) {
      this._inTransition = false;
    }
  }

  /**
   * Apply user-supplied color overrides on top of the system theme.
   * Pass null or an empty object to clear overrides and return to the
   * system theme.
   *
   * Accepted keys (all optional, values are "#rrggbb" hex strings):
   *   background, surface, text, text_muted, primary, error, success,
   *   focus_ring ("#rrggbbaa" with alpha supported for focus_ring).
   *
   * @param {object|null} overrides
   */
  setCustomOverrides(overrides) {
    this._customOverrides = (overrides && Object.keys(overrides).length > 0)
      ? overrides
      : null;
    // Re-apply immediately so the new overrides take effect this frame.
    this._applyInstant(this._targetDark);
  }

  // -- private ---------------------------------------------------------------

  _startTransition(toDark) {
    this._targetDark = toDark;

    if (this._reducedMotionMq.matches) {
      this._applyInstant(toDark);
      return;
    }

    const currentlyDark = this._inTransition
      ? this._transitionTo === "dark"
      : this._transitionFrom === "dark";

    this._transitionFrom = currentlyDark ? "dark" : "light";
    this._transitionTo   = toDark ? "dark" : "light";
    this._transitionStart = performance.now();
    this._inTransition = true;
  }

  _applyInstant(dark) {
    this._inTransition = false;
    this._transitionFrom = dark ? "dark" : "light";
    this._transitionTo   = dark ? "dark" : "light";
    this._applyProgress(dark ? 1.0 : 0.0);
  }

  /**
   * Push the current theme state into Wasm.
   * @param {number} progress  0.0 = fully light, 1.0 = fully dark
   */
  _applyProgress(progress) {
    // Snap to the nearest built-in theme.  A future improvement could add a
    // dedicated WASM binding for per-frame lerp to achieve pixel-perfect
    // interpolation, but snapping is sufficient for 300 ms transitions.
    this._app.set_theme(progress >= 0.5);

    if (this._customOverrides) {
      this._applyCustomOverrides(this._customOverrides);
    }
  }

  /**
   * Apply user-supplied hex overrides on top of the current WASM theme.
   * @param {object} overrides
   */
  _applyCustomOverrides(overrides) {
    const dark = this._targetDark;

    // Built-in defaults — filled when the caller omits a key.
    const D = dark ? {
      bg:         [0.102, 0.102, 0.102, 1.0],
      surface:    [0.145, 0.145, 0.145, 1.0],
      text:       [0.910, 0.910, 0.910, 1.0],
      text_muted: [0.565, 0.565, 0.565, 1.0],
      primary:    [0.350, 0.580, 0.980, 1.0],
      error:      [0.980, 0.360, 0.360, 1.0],
      success:    [0.270, 0.820, 0.380, 1.0],
      focus_ring: [0.350, 0.580, 0.980, 0.85],
    } : {
      bg:         [0.970, 0.970, 0.960, 1.0],
      surface:    [1.000, 1.000, 1.000, 1.0],
      text:       [0.100, 0.100, 0.120, 1.0],
      text_muted: [0.400, 0.400, 0.450, 1.0],
      primary:    [0.200, 0.450, 0.900, 1.0],
      error:      [0.880, 0.200, 0.200, 1.0],
      success:    [0.200, 0.700, 0.300, 1.0],
      focus_ring: [0.200, 0.450, 0.900, 0.80],
    };

    const resolve = (key, def) => {
      const val = overrides[key];
      if (val && typeof val === "string") {
        const parsed = hexToRgba(val);
        if (parsed) return parsed;
      }
      return def;
    };

    const bg      = resolve("background", D.bg);
    const surface = resolve("surface",    D.surface);
    const text    = resolve("text",       D.text);
    const tm      = resolve("text_muted", D.text_muted);
    const primary = resolve("primary",    D.primary);
    const error   = resolve("error",      D.error);
    const success = resolve("success",    D.success);
    const fr      = resolve("focus_ring", D.focus_ring);

    this._app.set_custom_theme(
      bg[0],      bg[1],      bg[2],
      surface[0], surface[1], surface[2],
      text[0],    text[1],    text[2],
      tm[0],      tm[1],      tm[2],
      primary[0], primary[1], primary[2],
      error[0],   error[1],   error[2],
      success[0], success[1], success[2],
      fr[0],      fr[1],      fr[2],      fr[3],
    );
  }
}

async function main() {
  await init();
  resize();
  const app = new WasmApp(canvas, canvas.width, canvas.height, dpr);
  window.__app = app;

  // Expose last frame time on window so external tooling or WASM can read it.
  Object.defineProperty(window, "__whamLastFrameMs", {
    get: () => lastFrameTimeMs,
    enumerable: true,
  });

  // Debug overlay — opt-in via `window.__WHAM_DEBUG = true` before init().
  const showFrameStats = !!(window.__WHAM_DEBUG);
  const frameStatsOverlay = showFrameStats ? createFrameStatsOverlay() : null;

  // --- Theme controller — prefers-color-scheme + app-level customization ---
  const themeCtrl = new ThemeController(app);

  /**
   * Global API for app-level theme customization.
   *
   * Accepts a JS object with optional "#rrggbb" hex color overrides:
   *   window.whamSetTheme({ primary: "#ff6600", background: "#0d0d0d" })
   *
   * Supported keys: background, surface, text, text_muted, primary, error,
   * success, focus_ring.  Pass null to clear overrides and revert to the
   * system theme.
   *
   * @param {object|null} themeConfig
   */
  window.whamSetTheme = (themeConfig) => {
    themeCtrl.setCustomOverrides(themeConfig);
  };

  const hiddenTextarea = createHiddenTextarea();
  const a11yMirror = new AccessibilityMirror(canvas, app, dpr);

  // --- Load fallback font (optional) ---
  // To support emoji or additional scripts, load a fallback font after the
  // primary font.  The fallback chain is tried in order when a glyph is
  // missing from the primary font.
  //
  // Example:
  //   fetch("NotoColorEmoji-Regular.ttf")
  //     .then(r => r.arrayBuffer())
  //     .then(buf => app.add_fallback_font(new Uint8Array(buf)));

  // --- IME composition state ---
  // Track whether an IME composition session is active so we can suppress
  // redundant text-input events that would cause double-insertion of the
  // composed string.  We maintain our own flag in addition to checking
  // e.isComposing on individual events because the relative ordering of
  // compositionend vs beforeinput differs across browsers (Chrome fires
  // compositionend first; Firefox fires beforeinput first).
  let isComposing = false;

  // --- WebGL context loss / restoration ---
  let contextLost = false;

  canvas.addEventListener("webglcontextlost", (e) => {
    e.preventDefault();
    contextLost = true;
    app.notify_context_lost();
    console.warn("[wham] WebGL context lost — rendering paused, form state preserved");
  });

  canvas.addEventListener("webglcontextrestored", () => {
    console.info("[wham] WebGL context restored — reinitializing renderer");
    try {
      app.reinitialize_renderer();
      contextLost = false;
    } catch (err) {
      console.error("[wham] Failed to reinitialize renderer after context restore:", err);
    }
  });

  // Send initial safe area insets before the first frame.
  updateSafeAreaInsets(app);

  window.addEventListener("resize", () => {
    resize();
    app.resize(canvas.width, canvas.height, dpr);
    // Safe area may change on resize (e.g. split-screen mode on iPad).
    updateSafeAreaInsets(app);
  });

  // Orientation change on phones can flip which edges have insets.
  window.addEventListener("orientationchange", () => {
    // Wait one rAF for the browser to reflow and update env() values.
    requestAnimationFrame(() => {
      resize();
      app.resize(canvas.width, canvas.height, dpr);
      updateSafeAreaInsets(app);
    });
  });

  // --- visualViewport resize (virtual keyboard open/close) ---
  if (window.visualViewport) {
    window.visualViewport.addEventListener("resize", () => {
      // When the virtual keyboard opens the visual viewport shrinks.  Scroll
      // the focused widget into the visible area so it is not hidden behind
      // the keyboard.
      const rect = app.focused_widget_rect();
      if (!rect) return;

      const widgetBottomCSS = rect[1] / dpr + rect[3] / dpr;
      const viewportHeight = window.visualViewport.height;
      const viewportOffsetTop = window.visualViewport.offsetTop;
      const visibleBottom = viewportOffsetTop + viewportHeight;

      if (widgetBottomCSS > visibleBottom) {
        const scrollBy = widgetBottomCSS - visibleBottom + 20; // 20px padding
        window.scrollBy({ top: scrollBy, behavior: "smooth" });
      }
    });
  }

  // --- Clipboard helpers ---
  //
  // Safari requires Clipboard API calls to happen synchronously within a
  // user-gesture (keydown / pointerdown) context.  If a Wasm call or an
  // async gap sits between the browser event and the clipboard call, Safari
  // revokes the transient activation and the write/read silently fails.
  //
  // Strategy:
  //   Copy (Ctrl/Cmd+C) & Cut (Ctrl/Cmd+X):
  //     1. Detect the shortcut in the keydown handler.
  //     2. Forward the key event to Wasm so it updates internal state (e.g.
  //        sets clipboard_request with the selected text).
  //     3. Immediately (still in the same event handler — synchronous with
  //        the user gesture) call take_clipboard_request() and write to the
  //        clipboard.
  //   Paste (Ctrl/Cmd+V):
  //     1. Detect the shortcut in the keydown handler.
  //     2. Read from the clipboard *first* (while still in the user gesture).
  //     3. Pass the result to Wasm via handle_paste().
  //     4. Suppress the default keydown so we don't double-paste from the
  //        native "paste" event listener.
  //
  // On browsers without navigator.clipboard (e.g. non-secure contexts) we
  // fall back to document.execCommand.

  /**
   * Write text to the system clipboard.
   *
   * Prefers the async Clipboard API (works in all modern browsers when called
   * inside a user gesture).  Falls back to execCommand('copy') via a
   * temporary textarea for older browsers or insecure contexts.
   *
   * @param {string} text
   */
  function clipboardWrite(text) {
    if (navigator.clipboard && typeof navigator.clipboard.writeText === "function") {
      navigator.clipboard.writeText(text).catch((err) => {
        if (typeof console !== "undefined" && console.warn) {
          console.warn("[wham] clipboard write failed:", err);
        }
        // Attempt execCommand fallback on permission failure.
        clipboardWriteFallback(text);
      });
      return;
    }
    clipboardWriteFallback(text);
  }

  /** execCommand('copy') fallback for environments without Clipboard API. */
  function clipboardWriteFallback(text) {
    const ta = document.createElement("textarea");
    ta.value = text;
    // Position off-screen so it is invisible but still selectable.
    Object.assign(ta.style, {
      position: "fixed",
      left: "-9999px",
      top: "-9999px",
      opacity: "0",
    });
    document.body.appendChild(ta);
    ta.select();
    try {
      document.execCommand("copy");
    } catch (err) {
      if (typeof console !== "undefined" && console.warn) {
        console.warn("[wham] execCommand('copy') fallback failed:", err);
      }
    }
    document.body.removeChild(ta);
  }

  /**
   * Read text from the system clipboard.
   *
   * Returns a Promise<string>.  Prefers the async Clipboard API, falls back
   * to execCommand('paste') (which only works in some browsers).
   */
  function clipboardRead() {
    if (navigator.clipboard && typeof navigator.clipboard.readText === "function") {
      return navigator.clipboard.readText().catch((err) => {
        if (typeof console !== "undefined" && console.warn) {
          console.warn("[wham] clipboard read failed:", err);
        }
        return "";
      });
    }
    // execCommand('paste') fallback — limited browser support.
    return Promise.resolve("");
  }

  /**
   * Returns true if the event represents a copy/cut/paste keyboard shortcut.
   * Accounts for Ctrl (Windows/Linux) and Cmd (macOS).
   */
  function isClipboardShortcut(e) {
    const mod = e.ctrlKey || e.metaKey;
    if (!mod) return null;
    const key = e.key?.toLowerCase();
    if (key === "c") return "copy";
    if (key === "x") return "cut";
    if (key === "v") return "paste";
    return null;
  }

  canvas.addEventListener("pointerdown", (e) => {
    // Capture the pointer so that pointermove/pointerup events continue to
    // fire even when the pointer leaves the canvas (e.g. during text
    // selection drag).  The capture is released automatically on pointerup
    // or via the explicit releasePointerCapture call below.
    canvas.setPointerCapture(e.pointerId);
    app.handle_pointer_down(e.offsetX * dpr, e.offsetY * dpr, e.button, e.ctrlKey, e.altKey, e.shiftKey, e.metaKey);
    // On touch devices, focus the hidden textarea so the virtual keyboard opens.
    if (e.pointerType === "touch") {
      hiddenTextarea.focus({ preventScroll: true });
    }
  });
  canvas.addEventListener("pointerup", (e) => {
    canvas.releasePointerCapture(e.pointerId);
    app.handle_pointer_up(e.offsetX * dpr, e.offsetY * dpr, e.button, e.ctrlKey, e.altKey, e.shiftKey, e.metaKey);
  });
  canvas.addEventListener("pointermove", (e) => {
    app.handle_pointer_move(e.offsetX * dpr, e.offsetY * dpr, e.ctrlKey, e.altKey, e.shiftKey, e.metaKey);
  });
  canvas.addEventListener("wheel", (e) => {
    app.handle_wheel(e.offsetX * dpr, e.offsetY * dpr, e.deltaX, e.deltaY, e.ctrlKey, e.altKey, e.shiftKey, e.metaKey);
    // Prevent the page from scrolling when the canvas has a focused widget.
    if (app.has_focused_widget()) {
      e.preventDefault();
    }
  }, { passive: false });

  window.addEventListener("keydown", (e) => {
    // During an active IME composition the OS/browser is in control of the
    // editing session.  Forwarding key events (arrows, backspace, etc.) to
    // Wasm would interfere with the composition window.
    if (e.isComposing || isComposing) {
      return;
    }

    const clipboardAction = isClipboardShortcut(e);

    if (clipboardAction === "copy" || clipboardAction === "cut") {
      // Forward the key event to Wasm first so it populates the clipboard
      // request (selected text) and, for cut, deletes the selection.
      app.handle_key_down(e.code, e.ctrlKey, e.altKey, e.shiftKey, e.metaKey);
      // NOTE: After calling into Wasm, any cached typed-array views into
      // wasm.memory.buffer may be detached (memory.grow).  We only read a
      // JS string from take_clipboard_request(), so no view re-acquisition
      // is needed here.

      // Still inside the user gesture — clipboard write will succeed on Safari.
      const text = app.take_clipboard_request();
      if (text) {
        clipboardWrite(text);
      }
      // Prevent the browser from firing its own copy/cut (there is no real
      // selection in the DOM to copy).
      e.preventDefault();
      return;
    }

    if (clipboardAction === "paste") {
      // Initiate the clipboard read while still inside the user gesture so
      // Safari grants permission.  The Wasm handle_paste() call happens in
      // the .then() callback — this is fine because Wasm does not require a
      // user gesture; only the browser clipboard API does.
      e.preventDefault();
      clipboardRead().then((text) => {
        if (text) {
          app.handle_paste(text);
        }
      });
      return;
    }

    app.handle_key_down(e.code, e.ctrlKey, e.altKey, e.shiftKey, e.metaKey);

    // Prevent browser defaults for keys that widgets handle when focused.
    // We check focus state AFTER forwarding to Wasm because the key event
    // may itself change focus (e.g. Tab moves focus to the next widget).
    if (app.has_focused_widget()) {
      const PREVENT_KEYS = new Set([
        "Tab", "Backspace", "Enter", "Space",
        "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight",
      ]);
      if (PREVENT_KEYS.has(e.key) || PREVENT_KEYS.has(e.code)) {
        e.preventDefault();
      }
    }
  });
  window.addEventListener("keyup", (e) => {
    app.handle_key_up(e.code, e.ctrlKey, e.altKey, e.shiftKey, e.metaKey);
  });
  window.addEventListener("beforeinput", (e) => {
    // During IME composition the intermediate text is managed by the
    // compositionupdate/compositionend handlers.  If we also forwarded the
    // beforeinput data we would insert the composed string twice.
    //
    // We check both our manual flag AND e.isComposing because:
    //   - Chrome fires compositionend *before* the final beforeinput, so
    //     isComposing is already false but e.isComposing is still true.
    //   - Firefox fires beforeinput *before* compositionend, so
    //     e.isComposing may be false but isComposing is still true.
    if (!e.isComposing && !isComposing && e.data) {
      app.handle_text_input(e.data);
    }
    // Prevent the browser from inserting text into the hidden textarea (or
    // any other focused DOM element) when a canvas text widget owns focus.
    const kind = app.focused_widget_kind();
    if (kind === "textinput" || kind === "select") {
      e.preventDefault();
    }
  });
  window.addEventListener("compositionstart", () => {
    isComposing = true;
    app.handle_composition_start();
  });
  window.addEventListener("compositionupdate", (e) => app.handle_composition_update(e.data || ""));
  window.addEventListener("compositionend", (e) => {
    isComposing = false;
    app.handle_composition_end(e.data || "");
  });

  // The native "paste" event fires when the user pastes via the browser
  // context menu or on mobile long-press.  Keyboard paste (Ctrl/Cmd+V) is
  // handled in the keydown listener above and preventDefault()ed so this
  // handler will not double-fire for keyboard paste.
  window.addEventListener("paste", (e) => {
    const text = e.clipboardData?.getData("text/plain") || "";
    if (text) app.handle_paste(text);
  });

  let prevFrameTs = 0;

  function frame(ts) {
    if (contextLost) {
      prevFrameTs = ts;
      requestAnimationFrame(frame);
      return;
    }

    // Measure frame duration. On the very first frame prevFrameTs is 0, so
    // we skip recording to avoid an anomalously large first sample.
    const frameMs = prevFrameTs > 0 ? ts - prevFrameTs : 0;
    prevFrameTs = ts;

    // Advance any in-progress theme transition before rendering so the Wasm
    // runtime receives updated theme colors this frame.
    themeCtrl.tick(ts);

    const a11y = app.frame(ts);
    // NOTE: After calling app.frame() any typed-array views into
    // wasm.memory.buffer may have been detached by memory.grow.
    // We only use the JS object `a11y` (not a typed-array view) so no
    // re-acquisition is needed here.
    window.__a11y = a11y;

    if (frameMs > 0) {
      recordFrameTime(frameMs);
      if (frameStatsOverlay) {
        frameStatsOverlay.update(frameMs);
      }
    }

    // Update the accessibility shadow DOM mirror.
    a11yMirror.update(a11y);

    // Sync focus from Wasm to the DOM mirror so screen readers track
    // the canvas focus state.
    if (a11y && a11y.root) {
      const focusedNode = (a11y.root.children || []).find(
        (n) => n.state && n.state.focused
      );
      if (focusedNode) {
        a11yMirror.syncFocus(focusedNode.id);
      }
    }

    // Drain any clipboard request that was produced outside a user gesture
    // (e.g. programmatic copy triggered by a button widget).  On Safari this
    // write may be rejected — clipboardWrite() logs a warning instead of
    // silently swallowing the error.
    const pendingClip = app.take_clipboard_request();
    if (pendingClip) {
      clipboardWrite(pendingClip);
    }

    // Reposition the hidden textarea over the currently focused widget so
    // that iOS Safari focus behaviour does not cause viewport scrolling.
    const focusedRect = app.focused_widget_rect();
    repositionTextarea(hiddenTextarea, focusedRect, dpr);

    requestAnimationFrame(frame);
  }
  requestAnimationFrame(frame);
}

main();
