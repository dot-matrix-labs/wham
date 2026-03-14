# Rust → WebAssembly Memory Constraints in Modern Browsers: Risks, Failure Modes, and Production Mitigations

**Abstract.** WebAssembly (Wasm) has matured from an experimental compilation target into a production-grade execution environment deployed by some of the largest software companies in the world. Rust, with its ownership model and zero-cost abstractions, has emerged as the dominant systems language for Wasm-targeted browser applications. This paper provides a systematic analysis of the memory constraints, failure modes, and architectural implications of Rust code compiled to WebAssembly and executed within modern browser engines. We construct a taxonomy of memory risks spanning Rust-specific hazards, Wasm-model hazards, and cross-boundary amplification effects. We examine GPU-side memory constraints for applications that render via WebGL or WebGPU, and analyze how the choice between immediate-mode and retained-mode rendering architectures shapes the memory profile and determines which mitigations are effective. We then present a layered mitigation playbook drawn from production deployments. The intended audience is systems engineers, browser platform engineers, and security researchers working with or evaluating Wasm-based architectures.

---

## 1. Motivation: Why Rust → WebAssembly

### 1.1 The Security Case for Compiled Wasm

1.1.1. The modern browser is the most widely deployed application runtime in existence. It is also one of the most hostile execution environments ever constructed: untrusted code from arbitrary origins executes within a shared process, mediated by a security model that has been repeatedly demonstrated to contain exploitable gaps. The decision to compile Rust to WebAssembly and deploy it in the browser is, in significant part, a security decision. Understanding the security properties this choice confers—and the new attack surfaces it introduces—is essential context for any discussion of its memory model.

1.1.2. Native browser plugin architectures (NPAPI, ActiveX, and their descendants) historically provided high performance at catastrophic security cost. These systems executed native C and C++ code with minimal sandboxing, exposing the full range of memory corruption vulnerabilities endemic to those languages:

- **Use-after-free.** Dangling pointer dereferences remain the single most exploited vulnerability class in browser engines and native plugins. CVE databases for Chrome, Firefox, and Safari consistently show use-after-free as the leading root cause of remote code execution.
- **Buffer overflows.** Stack and heap buffer overflows in native code have been exploitable since the Morris worm (1988) and remain exploitable in every C/C++ codebase that performs manual bounds management.
- **Uninitialized memory reads.** Reading from uninitialized stack or heap memory can leak sensitive data (cryptographic keys, session tokens, ASLR base addresses) to an attacker who controls subsequent use of the disclosed value.
- **Type confusion.** Incorrect casts in C++ vtable dispatch or union field access produce exploitable memory corruption when an object of one type is treated as another.
- **Data races.** Concurrent unsynchronized access to shared mutable state in multithreaded native code produces undefined behavior, which compilers are entitled to exploit in ways that break security invariants.
- **Undefined behavior from pointer misuse.** Strict aliasing violations, out-of-bounds pointer arithmetic, and null pointer dereferences all invoke undefined behavior in C and C++. Compilers may silently delete security checks that depend on defined behavior of these operations.

1.1.3. Rust eliminates or substantially mitigates every one of these vulnerability classes at the language level:

- The ownership and borrowing system prevents use-after-free and data races at compile time.
- All array and slice accesses are bounds-checked by default; unchecked access requires explicit `unsafe`.
- All memory is initialized before use; the compiler rejects programs that read uninitialized values.
- The type system is sound (modulo `unsafe`); there is no implicit casting or pointer arithmetic in safe Rust.
- The `Send` and `Sync` trait system prevents data races across thread boundaries.

1.1.4. When Rust code is compiled to WebAssembly, these language-level guarantees are preserved in the emitted bytecode. The Wasm execution environment then provides a second layer of defense: the linear memory sandbox.

### 1.2 JavaScript and Browser Attack Surfaces

1.2.1. JavaScript, as the incumbent browser programming language, carries its own distinct set of security risks. While it does not suffer from memory corruption in the C/C++ sense, its dynamic nature and deep integration with the browser platform create attack surfaces that Rust/Wasm avoids or reduces:

- **Supply chain vulnerabilities.** The npm ecosystem, which underpins nearly all JavaScript web applications, has been repeatedly compromised via dependency confusion, typosquatting, and malicious package injection (event-stream, ua-parser-js, colors/faker). A single compromised transitive dependency can exfiltrate credentials, inject cryptominers, or install backdoors. Rust's `crates.io` ecosystem is not immune, but Rust's compilation model means dependencies are statically linked and auditable at the source level, and the compiled Wasm binary contains no dynamic `require()` or `import()` resolution.
- **Prototype pollution.** JavaScript's prototype chain allows attackers to inject properties into `Object.prototype` or other built-in prototypes, modifying the behavior of all downstream code. This class of vulnerability has no equivalent in Rust.
- **DOM-based injection.** Cross-site scripting (XSS) via DOM manipulation remains one of the most prevalent web vulnerabilities. Rust/Wasm applications that render to a canvas or WebGPU surface bypass the DOM entirely, eliminating this class of attack.
- **Cross-origin data leakage.** Spectre-class side-channel attacks, timing attacks on `SharedArrayBuffer`, and CSS-based data exfiltration all exploit the browser's multi-origin execution model. Wasm linear memory is isolated from the JavaScript heap and from other origins' memory, providing a degree of side-channel resistance.
- **Logic bugs from dynamic typing.** JavaScript's implicit type coercion (`[] + {} === "[object Object]"`, `"0" == false`) causes classes of logic bugs that do not exist in statically typed languages. Rust's type system catches these errors at compile time.
- **Runtime performance unpredictability.** JavaScript's just-in-time compilation, garbage collection pauses, and deoptimization bailouts create execution time variability that can be exploited for timing side channels or denial-of-service. Wasm provides ahead-of-time compiled, deterministic execution with no GC pauses.

### 1.3 The Rust/Wasm Security Platform

1.3.1. Taken together, these properties position Rust→Wasm as a secure systems platform for large browser applications:

- **Memory safety without garbage collection.** Rust provides compile-time memory safety guarantees without the performance unpredictability of tracing garbage collection.
- **Deterministic performance.** Wasm executes ahead-of-time compiled machine code with predictable instruction timing, no JIT warmup, and no GC pauses.
- **Language-level safety.** Rust's type system, ownership model, and borrow checker eliminate entire classes of vulnerabilities at the language level rather than relying on runtime detection or sanitizers.
- **Sandbox isolation.** Wasm linear memory is isolated from the host environment. A Wasm module cannot access the JavaScript heap, the DOM, or the operating system except through explicitly imported host functions.
- **Reduced attack surface.** A Wasm module that renders to a canvas has no access to `document.cookie`, `localStorage`, `XMLHttpRequest`, or any other Web API unless the host explicitly provides it. This inverts the default-permissive model of JavaScript.

1.3.2. These are not theoretical benefits. They are the engineering reasons that the largest software companies in the world have adopted Rust→Wasm for their most performance-critical and security-sensitive browser workloads.

---

## 2. Evidence: Large-Scale Industry Adoption

### 2.1 Design Tools and Creative Applications

2.1.1. **Figma** is the canonical example of successful large-scale WebAssembly deployment. Figma's 2D rendering engine is implemented in C++ compiled to WebAssembly (originally via asm.js, later migrated to Wasm). The engine handles vector rendering, constraint solving, and layout computation for design documents that routinely contain thousands of objects across hundreds of frames. Figma's adoption of Wasm preceded broad browser support and helped drive the standardization process. Their rendering engine operates within a single Wasm linear memory that can grow to hundreds of megabytes for complex documents, and their engineering team has published extensively on the memory management challenges this entails.

2.1.2. **Adobe** has deployed WebAssembly in multiple products. Photoshop on the web uses Wasm (compiled from C++ via Emscripten) for image processing kernels, filter pipelines, and layer compositing. Adobe Acrobat on the web uses Wasm for PDF parsing, rendering, and form processing. Adobe Lightroom on the web uses Wasm for RAW image decoding and non-destructive editing operations. These deployments demonstrate that Wasm can support applications with memory footprints in the hundreds of megabytes, processing data sets (images, documents) that would be impractical to handle in JavaScript alone.

2.1.3. **Autodesk** has deployed WebAssembly for CAD (computer-aided design) workloads, including portions of their Fusion and AutoCAD web clients. CAD applications are particularly demanding because they require precise floating-point arithmetic, large geometric datasets (meshes with millions of vertices), and complex spatial indexing structures—all of which benefit from Rust or C++ memory layout control.

### 2.2 Edge Compute and Serverless Platforms

2.2.1. **Cloudflare Workers** uses the V8 Wasm runtime to execute user-provided Wasm modules at the edge. Cloudflare has deployed Wasm at a scale of millions of requests per second across hundreds of data centers. Their platform imposes strict memory limits on individual Wasm instances (typically 128 MB of linear memory) and relies on the Wasm sandbox for tenant isolation. This deployment has validated Wasm's isolation model under adversarial conditions: Cloudflare's customers include both the attackers and the defenders.

2.2.2. **Shopify** uses WebAssembly (via the Wasm runtime in their checkout extensibility platform) to sandbox third-party merchant extensions. This is a security-critical deployment: merchant extensions execute in the checkout flow, where they have access to cart data, pricing, and payment-adjacent state. The Wasm sandbox ensures that extensions cannot access memory outside their linear memory, cannot make unauthorized network requests, and cannot interfere with other extensions.

### 2.3 Productivity and Developer Tools

2.3.1. **Google** deploys WebAssembly in multiple products. Google Earth uses Wasm for 3D terrain rendering and geospatial computation. Google Meet uses Wasm for real-time background segmentation and noise cancellation (ML inference via TensorFlow Lite compiled to Wasm). Google Sheets uses Wasm for computational kernels. These deployments span a range of memory profiles: ML inference models may require 50–100 MB of linear memory for weight storage, while rendering engines may grow to several hundred megabytes.

2.3.2. Several code analysis and development tools have adopted Wasm for in-browser execution. Tree-sitter, the incremental parsing framework used by multiple code editors, compiles to Wasm for in-browser syntax highlighting and code navigation. Rust-analyzer has experimental Wasm builds. These tools demonstrate that complex, stateful, long-running computations can execute within the Wasm memory model.

### 2.4 Significance

2.4.1. These deployments collectively demonstrate several facts relevant to this paper:

- WebAssembly can support applications with working sets ranging from a few megabytes to several hundred megabytes.
- Production Wasm deployments routinely involve long-running sessions (hours) with complex state management.
- The Wasm memory model imposes real engineering constraints that these teams have had to solve through architectural patterns, not just code optimization.
- The security and isolation properties of Wasm are load-bearing in production—Cloudflare's and Shopify's entire extension security models depend on them.

---

## 3. WebAssembly Memory Model

### 3.1 Linear Memory

3.1.1. The fundamental memory abstraction in WebAssembly is **linear memory**: a contiguous, byte-addressable array of bytes. A Wasm module declares or imports a memory with an initial size (in pages) and an optional maximum size. The module can then load from and store to any byte offset within the bounds of the current memory size.

3.1.2. All memory addresses in Wasm are 32-bit unsigned integers (in the MVP specification; the memory64 proposal extends this to 64-bit). This means a Wasm module can address at most 4 GB of linear memory, regardless of the host system's available memory. In practice, browser implementations impose further limits (discussed in §3.3).

3.1.3. Linear memory is **zero-initialized**. When a module instantiates, all bytes in its initial memory are zero. When memory grows, the newly added pages are also zero-initialized. This provides a baseline security guarantee: Wasm modules cannot read uninitialized memory from previous tenants or previous instantiations.

### 3.2 Page-Based Allocation and `memory.grow`

3.2.1. Wasm linear memory is allocated in **pages** of 65,536 bytes (64 KB). The `memory.grow` instruction takes a delta (number of pages to add) and returns the previous memory size in pages, or -1 if growth failed.

3.2.2. Critically, **there is no `memory.shrink` instruction**. Once linear memory has grown, it cannot be reduced. The memory high-water mark is permanent for the lifetime of the module instance. This is the single most consequential architectural constraint of the Wasm memory model, and its implications pervade every section of this paper.

3.2.3. The grow-only model exists for sound engineering reasons:

- It ensures that all existing pointers remain valid after growth. If memory could shrink, any pointer into the upper region would become a dangling pointer, reintroducing use-after-free at the Wasm level.
- It simplifies the implementation of Wasm engines. The backing store can be implemented as a virtual memory reservation with guard pages; growth simply commits additional pages.
- It preserves the invariant that memory accesses within bounds can never trap due to concurrent modification of the memory size from another thread.

3.2.4. The consequence is that Wasm applications must either (a) tolerate permanently elevated memory usage after peak workloads, or (b) architect their systems to avoid peak memory spikes in the first place.

### 3.3 Interaction with Browser ArrayBuffer

3.3.1. In JavaScript host environments, Wasm linear memory is exposed as a `WebAssembly.Memory` object whose `buffer` property is an `ArrayBuffer` (or `SharedArrayBuffer` for shared memory). JavaScript code can create typed array views (`Uint8Array`, `Float32Array`, etc.) over this buffer to read from and write to Wasm memory.

3.3.2. A critical implementation detail: **when `memory.grow` is called, the backing `ArrayBuffer` is detached and replaced with a new, larger `ArrayBuffer`**. Any typed array views created over the old buffer become invalidated. Accessing a detached typed array view throws a `TypeError` in JavaScript. This is a frequent source of bugs in Wasm/JS interop code (discussed in §4.2).

3.3.3. The `ArrayBuffer` backing Wasm memory is subject to the browser's `ArrayBuffer` size limits. These limits vary by browser and platform:

- Chrome/V8 on 64-bit platforms: typically 4 GB (the Wasm32 address space limit).
- Chrome/V8 on 32-bit platforms or Android: often 1 GB or less due to virtual address space constraints.
- Firefox/SpiderMonkey: similar limits, with additional per-process memory budgets.
- Safari/JavaScriptCore: historically more conservative, with limits as low as 1 GB on some iOS versions due to the system's memory pressure policies.

3.3.4. These limits are **not standardized** and may change between browser versions. Applications that assume a specific maximum memory size may fail on platforms with lower limits.

### 3.4 Heap Allocators Inside Linear Memory

3.4.1. The Wasm specification provides only raw linear memory and the `memory.grow` instruction. It does not provide `malloc`, `free`, or any higher-level allocation API. Language runtimes must implement their own heap allocators within linear memory.

3.4.2. For Rust compiled to Wasm, the allocator landscape is as follows:

- **dlmalloc** (the `wee_alloc` replacement and default in many configurations): A port of Doug Lea's malloc, adapted for Wasm. It manages free lists and bins within linear memory and calls `memory.grow` when it needs more pages.
- **wee_alloc** (deprecated but still encountered): A deliberately small (~1 KB code size) allocator designed for code-size-sensitive Wasm deployments. Known for poor fragmentation behavior and inability to return memory to the system.
- **talc**: A more recent allocator designed specifically for Wasm, with better fragmentation characteristics.
- **Custom arena allocators**: Many production Wasm applications bypass the general-purpose allocator entirely for hot paths, using arena or bump allocators (discussed in §7.3).

3.4.3. All of these allocators face a fundamental constraint: they can request more pages from `memory.grow`, but they **cannot return pages to the Wasm runtime**. When a Rust program calls `dealloc`, the allocator marks the memory as available for future allocations within its own data structures, but the underlying Wasm pages remain committed. From the browser's perspective, the memory is still in use.

### 3.5 Browser Engine Implementation Details

3.5.1. **V8** (Chrome, Edge, Node.js) implements Wasm linear memory using virtual memory reservations. On 64-bit platforms, V8 reserves the full 4 GB virtual address range for each Wasm memory (plus guard pages) and commits physical pages on demand as `memory.grow` is called. This means that `memory.grow` is relatively cheap (a `mprotect` or equivalent) but the virtual address space reservation is large.

3.5.2. **SpiderMonkey** (Firefox) uses a similar virtual memory reservation strategy. SpiderMonkey additionally supports asm.js-style signal-handler-based bounds checking on supported platforms, which eliminates explicit bounds check instructions at the cost of requiring large virtual memory reservations with guard regions.

3.5.3. **JavaScriptCore** (Safari, WebKit) has historically been more conservative with virtual memory reservations, which has occasionally resulted in lower effective memory limits on memory-constrained platforms (particularly iOS, where the system aggressively kills background processes that exceed memory budgets).

---

## 4. Taxonomy of Memory Risks

### 4.1 Rust-Exclusive Memory Risks

These are memory hazards that can occur in Rust/Wasm systems but **cannot occur** in pure JavaScript applications, because they depend on the ability to perform unsafe memory operations within compiled code.

4.1.1. **Unsafe Rust memory corruption.** Rust's safety guarantees are predicated on the assumption that `unsafe` blocks uphold the language's safety invariants. When they do not, the resulting undefined behavior can manifest as memory corruption within Wasm linear memory. Common causes include:

- Incorrect pointer arithmetic in `unsafe` blocks.
- Transmuting between types with different validity requirements (`std::mem::transmute`).
- Creating multiple mutable references to the same data (aliasing violations).
- Calling `std::ptr::read` or `std::ptr::write` on improperly aligned or out-of-bounds pointers.

4.1.2. Within the Wasm sandbox, such corruption cannot escape linear memory—it cannot corrupt the browser's heap or gain code execution on the host. But it can corrupt the application's own state, leading to incorrect output, crashes (Wasm traps), or data loss. In safety-critical applications (document editors, financial tools), this is a production incident even without a security breach.

4.1.3. **Incorrect FFI bindings.** Rust/Wasm applications frequently interoperate with JavaScript via FFI bindings generated by tools such as `wasm-bindgen`, `wasm-pack`, or manual `extern "C"` declarations. Incorrect bindings can cause:

- Misinterpreted pointer sizes (e.g., treating a `usize` as a JavaScript `number` without accounting for 32-bit Wasm addressing).
- Incorrect ownership transfer (JavaScript holding a reference to Wasm memory that Rust has already freed).
- Type mismatches between the Rust signature and the JavaScript call site.

4.1.4. **Allocator corruption.** Because Rust allocators manage their own metadata (free lists, bin headers, chunk boundaries) within Wasm linear memory, a wild write from `unsafe` code can corrupt allocator metadata. This can cause subsequent allocations to return overlapping regions, leading to silent data corruption that manifests far from the original bug.

4.1.5. **Pointer aliasing violations.** Rust's compiler exploits the guarantee that mutable references are unique for optimization purposes (analogous to C's strict aliasing, but enforced by the borrow checker for safe code). In `unsafe` code, violating this guarantee can cause the compiler to emit code that reads stale values from registers or reorders stores in ways that corrupt data structures. The Miri interpreter can detect many such violations, but not all `unsafe` code is routinely tested under Miri, particularly in third-party crate dependencies.

### 4.2 WebAssembly-Specific Memory Issues

These are issues caused specifically by the Wasm memory model, independent of the source language.

4.2.1. **Linear memory high-water mark.** As discussed in §3.2, Wasm linear memory can only grow. A transient spike in memory usage—processing a large image, parsing a complex document, running a computationally intensive algorithm—permanently increases the memory footprint. In long-running applications (a design tool session lasting hours), this means memory usage monotonically increases over time even if the active working set remains stable.

4.2.2. **Stale typed-array views.** When `memory.grow` is called (either directly or indirectly via the Rust allocator), all existing JavaScript `TypedArray` views over the Wasm memory become detached. Any JavaScript code that caches a `Uint8Array` view across a call boundary that might trigger allocation will silently use an invalidated view. Depending on the browser, this may throw a `TypeError`, return `undefined`, or (historically) read from freed memory.

- This is a particularly insidious bug because it depends on allocation behavior: a code path that works correctly for small inputs (no `memory.grow` triggered) may fail for large inputs (where the allocation causes growth). This makes the bug difficult to reproduce in testing and likely to appear first in production with real-world data.

4.2.3. **Grow-only memory model.** The inability to shrink memory means that standard memory management strategies—releasing memory back to the OS after use—do not apply. In native applications, `munmap` or `VirtualFree` returns physical pages to the operating system; in Wasm, there is no equivalent. The only way to "free" Wasm memory is to destroy the entire module instance and create a new one.

4.2.4. **Pointer/offset interop between JS and Wasm.** Wasm memory addresses are byte offsets into linear memory. JavaScript code that interacts with Wasm data must convert between JavaScript values and Wasm offsets. Common errors include:

- Treating Wasm pointers as JavaScript object references (they are integers).
- Performing arithmetic on Wasm offsets using JavaScript's floating-point `Number` type, which loses precision for offsets above 2^53 (though this is unlikely in 32-bit Wasm, it can occur in intermediate calculations).
- Failing to account for alignment requirements when creating typed array views at arbitrary offsets.

4.2.5. **Allocator fragmentation inside linear memory.** Because the allocator cannot return pages to the runtime, fragmentation within linear memory is permanent. A workload that alternates between allocating large and small objects can produce a memory layout where free space exists in many small fragments, none large enough to satisfy a subsequent large allocation. The allocator must then call `memory.grow` even though the total free space exceeds the requested allocation. This phenomenon—the linear memory growing well beyond the actual live data—is the Wasm equivalent of heap fragmentation in native applications, but worse because the fragmented space can never be reclaimed.

4.2.6. **Assumptions of stable addresses.** Some data structures assume that allocated objects will not move. In Wasm, objects within linear memory do not move (there is no compacting GC), so this assumption holds. However, some interop patterns involve copying data between JavaScript objects and Wasm linear memory, and the JavaScript GC can move JavaScript objects. If a Wasm module holds a reference (via a handle or index) to a JavaScript-side object, and the JavaScript side reallocates or replaces that object, the Wasm module's reference becomes stale in a different sense—not a dangling pointer, but a semantic invalidation.

### 4.3 Issues That Exist in JavaScript but Worsen with Wasm

These are problems present in both ecosystems but amplified by the introduction of a Wasm module alongside JavaScript.

4.3.1. **Cross-boundary copying.** Transferring data between JavaScript and Wasm requires copying: JavaScript strings must be encoded to UTF-8 and written into Wasm linear memory; Wasm results must be read out and converted back to JavaScript types. For large data (images, documents, audio buffers), this copying can double memory usage during the transfer and introduce latency proportional to the data size.

- In pure JavaScript, large data can often be passed by reference (e.g., `ArrayBuffer` transfer). In Wasm interop, copying is frequently unavoidable because the Wasm module expects data at specific offsets within its linear memory.

4.3.2. **Dual heap duplication.** A Wasm application running in a browser has two heaps: the JavaScript heap (managed by the browser's garbage collector) and the Wasm linear memory (managed by the Rust allocator). Any data structure that must be accessible from both sides must either be duplicated (one copy in each heap) or mediated through a single source of truth with accessor functions. In practice, many applications duplicate data, doubling the memory cost of shared state.

4.3.3. **Callback lifetime leaks.** JavaScript closures passed as callbacks to Wasm functions can create reference cycles that prevent garbage collection. A common pattern: JavaScript registers a callback with Wasm; Wasm stores a reference (via `wasm-bindgen`'s `Closure` type) to the JavaScript function; the JavaScript function captures a reference to a DOM element or a large data structure. The Wasm-side reference prevents the JavaScript GC from collecting the closure and its captured state. Over time, this leaks memory on the JavaScript heap.

- `wasm-bindgen`'s `Closure` type requires explicit `.forget()` or manual dropping. The `.forget()` method intentionally leaks the closure to avoid the complexity of prevent the associated Wasm-side destructor running. This is a well-documented footgun that production applications must carefully manage.

4.3.4. **Serialization/deserialization overhead.** Complex data structures (trees, graphs, nested objects) cannot be shared by reference between JavaScript and Wasm. They must be serialized (typically to JSON, MessagePack, or a custom binary format) on one side and deserialized on the other. This process allocates temporary buffers on both heaps, and the peak memory usage during serialization can be several times the size of the data structure itself.

4.3.5. **Large temporary buffers.** Image processing, audio processing, and computational workloads frequently require large temporary buffers. In pure JavaScript, these buffers are garbage-collected when no longer referenced. In Wasm, they are freed to the allocator but the underlying pages remain committed (per §3.2). An application that occasionally processes a 100 MB image will permanently retain 100 MB of linear memory even if subsequent operations use only 1 MB.

4.3.6. **Stack exhaustion.** Wasm modules have a fixed-size call stack (separate from linear memory in most implementations, though some configurations place it within linear memory). Deep recursion or large stack-allocated arrays can exhaust the Wasm stack, producing a trap. The default stack size varies by engine (V8 defaults to ~1 MB for Wasm) and cannot be resized at runtime. This is a more immediate failure mode than in JavaScript, where stack overflow produces a catchable `RangeError`; in Wasm, a stack overflow is an unrecoverable trap.

---

## 5. GPU Memory Constraints

The preceding sections focus exclusively on Wasm linear memory. However, applications that render UI via WebGL or WebGPU—which is the entire premise of GPU-accelerated Wasm UI frameworks—maintain a second, largely invisible memory domain: GPU-side allocations. These allocations are not reflected in Wasm linear memory metrics, are not subject to the same grow-only constraints, and have their own failure modes that the paper must address.

### 5.1 The GPU Memory Domain

5.1.1. A GPU-rendered Wasm application manages memory across three distinct heaps: (1) Wasm linear memory (the Rust heap), (2) the JavaScript/browser heap, and (3) GPU memory managed by the WebGL/WebGPU driver. The paper's taxonomy (§4) covers only the first two. GPU memory is allocated through API calls (`gl.texImage2D`, `gl.bufferData`, `device.createBuffer`, `device.createTexture`) and lives outside Wasm linear memory entirely. Browser DevTools memory panels typically do not report GPU allocations alongside Wasm memory, making GPU memory leaks invisible to the monitoring strategies described in §7.1 of the mitigation playbook.

5.1.2. GPU memory is a shared, contention-prone resource. Unlike Wasm linear memory (which is per-instance and isolated), GPU memory is shared across all tabs, all contexts, and the compositor. On mobile devices with unified memory architectures (most ARM SoCs), GPU allocations compete directly with CPU allocations for the same physical RAM. A Wasm application that monitors only its linear memory usage may believe it is within budget while its GPU allocations push the device into memory pressure, causing the OS to kill background tabs or the application itself.

### 5.2 Texture Memory

5.2.1. **Texture atlases** are the dominant GPU memory consumer in 2D UI rendering. A forms-focused UI framework must maintain atlases for glyph caches (rendered text), icon sheets, and potentially image content. A single RGBA texture atlas at 2048×2048 consumes 16 MB of GPU memory. At 4096×4096 (common for high-DPI glyph caches), it consumes 64 MB. Multiple atlases—one for glyphs, one for icons, one for UI chrome—can easily exceed the Wasm linear memory footprint of the application logic itself.

5.2.2. **Glyph cache growth** is particularly dangerous for form-heavy applications. Each unique (font, size, weight, glyph) combination occupies atlas space. A form with multiple font styles, multilingual content (Latin + CJK glyphs), and dynamic sizing can exhaust a glyph atlas quickly. Unlike Wasm linear memory, texture atlases *can* be destroyed and recreated—but doing so mid-frame produces visual artifacts. The glyph cache eviction strategy directly affects both memory usage and rendering correctness.

5.2.3. **Texture upload latency.** Uploading texture data from Wasm linear memory to the GPU (`texImage2D`, `texSubImage2D`, `queue.writeTexture`) requires the data to exist simultaneously in Wasm linear memory (source) and GPU memory (destination). For large atlas updates, this doubles the peak memory cost of the texture data. Streaming uploads (updating sub-regions) mitigate this but add implementation complexity.

### 5.3 Buffer Objects

5.3.1. **Vertex and index buffers** for UI rendering are typically small per-frame (a forms UI draws hundreds to low thousands of quads), but buffer management strategy matters. Allocating new GPU buffers every frame and relying on garbage collection to reclaim old ones produces GPU memory churn and eventual pressure. Persistent buffers that are updated via `bufferSubData` or mapped writes avoid this but require careful sizing.

5.3.2. **Uniform buffers and bind groups** (WebGPU) or uniform uploads (WebGL) consume GPU memory proportional to the number of distinct materials, transforms, or rendering states. A UI framework with per-widget styling (colors, borders, shadows) may generate more uniform data than expected.

### 5.4 GPU Context Loss

5.4.1. WebGL contexts can be lost at any time due to GPU memory pressure, driver crashes, or system policies (notably on mobile where the OS reclaims GPU resources from backgrounded tabs). A lost context invalidates **all** GPU resources: textures, buffers, shaders, framebuffers. The application must be able to recreate its entire GPU state from data retained in Wasm linear memory or JavaScript.

5.4.2. This has a direct architectural implication: GPU resources cannot be the sole source of truth for any application state. The glyph cache, texture atlases, and rendering state must be reconstructable from CPU-side data. This means the application effectively maintains a shadow copy of its GPU state, increasing total memory usage but providing resilience against context loss. Applications that fail to handle context loss will render a black screen or crash after a GPU memory pressure event—a common failure on mobile browsers that the paper's risk taxonomy should include.

5.4.3. WebGPU's `device.lost` promise provides a cleaner recovery path than WebGL's context loss events, but the fundamental constraint is the same: GPU memory is ephemeral and the application must plan for total loss.

### 5.5 GPU Memory Budgeting

5.5.1. There is no reliable cross-browser API for querying available GPU memory. The `WEBGL_debug_renderer_info` extension reveals the GPU vendor and model (from which memory can be heuristically estimated), but provides no runtime usage data. Applications must set conservative GPU memory budgets based on target device profiles:

- Low-end mobile (Adreno 610, Mali-G57): ~1–2 GB shared CPU/GPU, effective GPU budget 100–200 MB.
- Mid-range mobile (Adreno 730, Mali-G710): ~4–6 GB shared, effective GPU budget 200–500 MB.
- Desktop (discrete GPU): 2–16 GB dedicated VRAM, but browser tab limits apply.

5.5.2. For a forms-focused application targeting mid-range mobile (per the project's success criteria), a practical GPU memory budget is: one 2048×2048 glyph atlas (16 MB), one 1024×1024 icon atlas (4 MB), persistent vertex/index buffers (< 1 MB), and shader programs (< 5 MB compiled). Total: ~25–30 MB. This is modest but must be actively managed—glyph cache eviction, atlas packing efficiency, and buffer reuse are not optional optimizations but correctness requirements on constrained devices.

---

## 6. Rendering Architecture and Memory Implications

The choice between immediate-mode and retained-mode rendering is the single most consequential architectural decision for the memory profile of a GPU-rendered Wasm UI application. This section analyzes both approaches and their interaction with the Wasm memory constraints described in §§3–5.

### 6.1 Immediate-Mode Rendering

6.1.1. In an immediate-mode architecture (exemplified by Dear ImGui, egui, and Makepad), the application reconstructs the entire UI description every frame. There is no persistent widget tree. The application calls drawing functions (`button("Submit")`, `text_input("Name", &mut name)`) that both define the UI and handle interaction in a single pass. The output is a list of draw commands (vertex data, texture references, scissor rects) that is submitted to the GPU and discarded.

6.1.2. **Memory characteristics of immediate mode:**

- **No persistent widget tree.** Memory usage is proportional to the visible UI, not the total UI. A 500-field form where only 20 fields are visible allocates memory only for the 20 visible fields. This is a natural form of virtualization.
- **Arena allocation is the natural fit.** The per-frame draw list is allocated into an arena that is reset at frame end. This perfectly aligns with the arena mitigation strategy (§7.3.1) and eliminates fragmentation from UI allocations. The Wasm linear memory high-water mark from UI rendering is bounded by the worst-case single frame, not accumulated over the session.
- **Lower steady-state memory.** No widget objects, no layout caches, no style resolution trees, no event dispatch tables. The only persistent state is the application's data model.
- **Higher per-frame CPU cost.** Rebuilding the UI every frame means layout computation, text measurement, and hit-testing happen every frame. On a 120Hz display (8.3ms frame budget), this can be expensive for complex forms.
- **Predictable memory profile.** Memory usage is bounded and periodic: it spikes during frame construction, drops to baseline after frame submission. This is the "predictable memory phases" pattern (§7.6.3) achieved by default rather than by careful engineering.

6.1.3. **Risks specific to immediate mode:**

- **Text measurement cost.** Measuring text layout every frame is expensive. Immediate-mode frameworks typically cache text measurements, which reintroduces persistent state and its associated memory. The cache must be bounded or it becomes a leak.
- **GPU buffer churn.** Generating new vertex data every frame means uploading new vertex buffers to the GPU every frame. On low-end GPUs, this upload bandwidth can become the bottleneck rather than memory.
- **State management complexity.** Without a widget tree, state that "belongs to" a widget (scroll position, animation progress, focus state) must be stored externally, typically in hash maps keyed by widget identity. These maps grow over the session and can leak if widget identities are not stable.

### 6.2 Retained-Mode Rendering

6.2.1. In a retained-mode architecture (exemplified by Flutter, the browser DOM, Druid/Xilem, and most traditional UI toolkits), the application constructs a persistent tree of widget objects. The framework diffs the new tree against the previous tree, computes a minimal set of changes, and updates the rendering accordingly.

6.2.2. **Memory characteristics of retained mode:**

- **Persistent widget tree.** Memory usage is proportional to the total UI, not the visible UI (unless explicit virtualization is implemented). A 500-field form allocates 500 widget nodes, their layout data, style data, and accessibility metadata, regardless of how many are on screen. For a forms application, this is typically tens to low hundreds of KB—manageable but non-trivial.
- **Layout caches.** Retained-mode frameworks cache layout results (position, size, baseline) per widget. This avoids recomputation but adds per-widget memory overhead. For complex layouts with constraints (flexbox-like), the cache may include constraint inputs and intermediate results.
- **Style resolution data.** If the framework supports theming or cascading styles, each widget may store resolved style properties. This adds per-widget overhead proportional to the number of style properties.
- **Diffing intermediaries.** Tree diffing algorithms (reconciliation) allocate temporary data structures to compare old and new trees. This creates periodic memory spikes during UI updates that contribute to the high-water mark.
- **Fragmentation risk.** Widgets are individually heap-allocated and have varied lifetimes (some persist for the session, others are created and destroyed as the user navigates). This allocation pattern—many small, variably-lived objects—is the worst case for heap fragmentation in a grow-only memory model.

6.2.3. **Risks specific to retained mode:**

- **Widget lifecycle leaks.** A retained widget tree can leak memory when widgets are "removed" from the visible tree but retain references (event handlers, animation controllers, data bindings) that prevent deallocation. This is the Wasm analog of DOM detached-node leaks in browsers, but harder to diagnose because Wasm has no equivalent of Chrome's "Detached DOM elements" heap snapshot filter.
- **Unbounded tree growth.** Dynamic forms that add and remove fields over a session (conditional sections, repeatable groups) cause the widget tree to churn. Even if widgets are correctly deallocated, the allocator fragmentation from repeated create/destroy cycles permanently inflates linear memory.
- **Virtualization is essential, not optional.** A retained-mode framework that does not virtualize long lists or large forms will allocate proportionally to total content. For a 500-field form, this is manageable; for a data table with 10,000 rows, it is not. Virtualization (rendering only visible rows with placeholder measurements for off-screen rows) is a correctness requirement, not a performance optimization.

### 6.3 Hybrid Approaches

6.3.1. Several modern frameworks adopt hybrid strategies:

- **Xilem** (Rust) uses a retained widget tree but reconstructs the view description every frame (like immediate mode), then diffs against the retained tree. This gives the ergonomics of immediate mode with the rendering efficiency of retained mode, but the memory profile is closer to retained mode because the persistent tree exists.
- **Makepad** (Rust/Wasm) uses an immediate-mode API with GPU-side retained state (persistent vertex buffers, cached draw calls). This inverts the typical trade-off: CPU-side memory is immediate-mode-minimal while GPU-side memory is retained.
- **React-like reconciliation in Rust** (Dioxus, Leptos, Sycamore) uses a virtual DOM or signal graph with diffing. These have retained-mode memory characteristics plus the overhead of the diffing data structures.

6.3.2. For a Wasm forms framework operating under the memory constraints of §§3–5, the choice has concrete implications:

| Concern | Immediate Mode | Retained Mode |
|---|---|---|
| Wasm linear memory baseline | Lower (no widget tree) | Higher (persistent tree) |
| Peak memory during interaction | Bounded per-frame | Spikes during reconciliation |
| Fragmentation risk | Low (arena per frame) | High (varied-lifetime allocations) |
| GPU memory profile | Higher churn (new buffers/frame) | Lower churn (incremental updates) |
| Glyph cache pressure | Same | Same |
| Suitability for arena allocation | Excellent | Poor (long-lived objects) |
| Memory predictability | High | Depends on implementation |

### 6.4 Recommendation for This Project

6.4.1. Given the project's constraints—forms-first, targeting mid-range mobile at 60fps, Wasm + WebGL/WebGPU rendering, <500KB bundle—the rendering architecture should be chosen with memory as a primary design axis, not just performance:

6.4.2. **An immediate-mode or immediate-mode-hybrid architecture is strongly favored** for the following memory-specific reasons:

- Arena allocation per frame eliminates the fragmentation problem that is the paper's central concern (§4.2.5). In a grow-only memory model, avoiding fragmentation is more valuable than any mitigation strategy for managing it.
- The forms use case is low to moderate complexity per frame (tens to hundreds of widgets visible), well within immediate-mode CPU budgets even at 120Hz.
- GPU buffer uploads for a forms UI are small (a few thousand vertices per frame). The buffer churn cost of immediate mode is negligible for this workload.
- No widget lifecycle leaks are possible when there are no persistent widget objects.
- Memory predictability—bounded, periodic, resettable—is achieved by default.

6.4.3. **The retained-mode approach is viable but requires more defensive engineering:**

- Per-widget allocation should use a typed arena or object pool, not the general-purpose allocator, to control fragmentation.
- Virtualization must be implemented from day one for any scrollable content.
- Widget lifecycle must be rigorously managed with explicit destroy phases and leak detection instrumentation.
- The memory budgeting strategy (§7.5.2) becomes essential rather than aspirational.

6.4.4. **GPU memory management applies equally to both architectures.** Regardless of rendering mode, the application must:

- Implement glyph cache eviction (LRU or LFU) with a bounded atlas size.
- Handle WebGL context loss and full GPU state reconstruction.
- Budget GPU memory separately from Wasm linear memory.
- Use persistent, pre-sized vertex/index buffers rather than per-frame allocation where possible (even in immediate mode, the *GPU-side* buffers should be retained and overwritten, not recreated).

---

## 7. Mitigation Strategies

The following mitigations are organized from lowest barrier to adoption (tooling and configuration changes) to highest (fundamental architectural redesign). Production Wasm applications typically employ strategies from multiple levels simultaneously.

### 7.1 Tooling Choices

7.1.1. **Browser DevTools memory profilers.** Chrome DevTools provides a "Memory" tab that can profile both the JavaScript heap and Wasm linear memory. The heap snapshot view shows JavaScript objects, while the "Memory" allocation timeline can track `ArrayBuffer` growth (which corresponds to Wasm linear memory growth). Firefox's memory tool provides similar capabilities. Engineers should use these tools to establish baseline memory profiles for typical workloads and identify unexpected growth.

7.1.2. **`wasm-objdump` and `wasm-dis`.** The WebAssembly Binary Toolkit (WABT) provides tools for inspecting compiled Wasm modules. `wasm-objdump -x` shows the memory section, including initial and maximum memory declarations. This is useful for verifying that the compiled module declares sensible memory bounds.

7.1.3. **Rust allocator instrumentation.** Rust's `GlobalAlloc` trait allows wrapping the allocator with instrumentation. A custom global allocator can track:

- Total bytes allocated and freed.
- Number of active allocations.
- Peak memory usage.
- Allocation call sites (via `#[track_caller]` or backtrace capture).

This instrumentation adds runtime overhead but provides application-level memory visibility that browser tools cannot. It is particularly valuable for identifying which subsystems are responsible for memory growth.

7.1.4. **Twiggy.** The `twiggy` tool analyzes compiled Wasm binaries to identify which functions and data sections contribute most to code size. While primarily a code-size tool, it can also reveal unexpectedly large static data segments or bloated generic instantiations that increase the module's memory baseline.

7.1.5. **`console.memory` and `performance.measureUserAgentSpecificMemory()`.** These JavaScript APIs provide programmatic access to memory metrics. `performance.measureUserAgentSpecificMemory()` (available in Chrome behind cross-origin isolation) reports per-origin memory usage including Wasm memory. This enables automated memory regression testing.

### 7.2 Language and Library Choices

7.2.1. **Minimize `unsafe` code.** Every `unsafe` block is a potential source of the Rust-exclusive memory risks described in §4.1. Production Wasm applications should:

- Audit all `unsafe` blocks and document their safety invariants.
- Prefer safe abstractions from well-audited crates over custom `unsafe` implementations.
- Use `#[forbid(unsafe_code)]` in modules that do not require `unsafe`.
- Run `cargo clippy` with all lints enabled to catch common `unsafe` antipatterns.

7.2.2. **Allocator selection.** The choice of allocator significantly affects fragmentation behavior and peak memory usage:

- `dlmalloc` (the default in many Wasm toolchains) provides reasonable general-purpose performance but can suffer from fragmentation in long-running workloads with varied allocation sizes.
- `talc` is designed for Wasm and provides better fragmentation resistance through a different free-list strategy.
- Custom allocators (arena, bump, pool) should be used for hot paths with known allocation patterns (§7.3).

7.2.3. **Avoid unnecessary serialization.** The interop boundary between JavaScript and Wasm is a major source of memory amplification (§4.3). Strategies to minimize interop copying include:

- Use shared `ArrayBuffer` views where possible, avoiding full copies.
- Design Wasm APIs that accept byte offsets and lengths rather than copying data in and out.
- Use zero-copy deserialization formats (e.g., FlatBuffers, Cap'n Proto) instead of JSON or MessagePack.
- Batch multiple small interop calls into single bulk transfers.

7.2.4. **Stable interop APIs.** Define a narrow, stable ABI between JavaScript and Wasm. Each function in this ABI should have documented ownership semantics: does the Wasm function take ownership of the input buffer (and free it), or does the caller retain ownership? Ambiguous ownership at the interop boundary is the leading cause of double-free and use-after-free bugs in production Wasm applications.

### 7.3 Architecture and Design Patterns

7.3.1. **Arena allocation.** An arena (also called a region or zone) allocator pre-allocates a large contiguous block and provides fast bump-pointer allocation within that block. When the arena is "reset," all objects in the arena are freed simultaneously by resetting the bump pointer to the beginning—no per-object deallocation is needed. This pattern is transformative for Wasm applications because:

- It eliminates fragmentation within the arena.
- It makes deallocation O(1) regardless of the number of objects.
- It naturally aligns with request/response or frame-based processing models.
- It reduces the frequency of `memory.grow` calls because the arena is reused.

7.3.2. **Scratch buffers.** Pre-allocate a set of reusable buffers for temporary data (image tiles, text encoding buffers, computation intermediaries). Rather than allocating and freeing buffers per operation, reuse the same buffers for each operation. This caps the memory overhead of temporary data at a fixed, predictable amount.

7.3.3. **Streaming processing.** Instead of loading an entire dataset into memory, process it in chunks:

- Image processing: process tiles rather than full images.
- Document parsing: use SAX-style event-driven parsing rather than DOM-style tree construction.
- Data transformation: process records in batches rather than materializing the full dataset.

Streaming reduces peak memory usage and avoids the linear-memory high-water-mark problem.

7.3.4. **Chunked workloads.** For computationally intensive operations, break the work into chunks that can be processed incrementally. Between chunks, reuse temporary allocations. This prevents a single large computation from permanently inflating the linear memory.

7.3.5. **Worker lifecycle resets.** For applications where memory growth is unavoidable, run the Wasm module in a Web Worker and periodically terminate and recreate the worker. This is the only way to truly "free" Wasm linear memory: destroying the Wasm instance releases the backing `ArrayBuffer`. The new worker starts with a fresh, minimal linear memory. This pattern requires serializing and transferring essential state to the new worker, which adds complexity but provides a hard upper bound on memory growth.

7.3.6. **Handle-based APIs.** Instead of exposing Wasm pointers to JavaScript, expose integer handles (indices into an internal table). The Wasm module maintains a table mapping handles to internal pointers. This decouples the JavaScript code from the Wasm memory layout and prevents stale-pointer bugs when internal data structures are reorganized.

### 7.4 Testing and Verification Patterns

7.4.1. **Fuzz testing.** Run `cargo fuzz` (which uses libFuzzer) against Wasm-targeted code. Fuzz testing is particularly effective at finding `unsafe` memory bugs because it can explore code paths that unit tests miss. For Wasm-specific testing, fuzz the interop boundary: generate random sequences of JavaScript→Wasm calls with random inputs and verify that the module does not trap unexpectedly.

7.4.2. **Property testing.** Use `proptest` or `quickcheck` to test invariants of data structures and algorithms. Property tests can verify, for example, that a data structure's size (in allocator bytes) is proportional to the number of elements it contains, catching memory leaks that would be invisible to functional tests.

7.4.3. **Load testing with large datasets.** Test with inputs at or above production scale. Many Wasm memory bugs are triggered only by inputs large enough to cause `memory.grow`. If the test suite uses only small inputs, these bugs will never be caught.

7.4.4. **Stress testing memory growth.** Write tests that deliberately trigger `memory.grow` and verify correct behavior:

- Allocate and free memory in patterns that maximize fragmentation.
- Grow memory to the declared maximum and verify graceful failure (not a trap) when further growth is requested.
- Verify that JavaScript interop code correctly handles `ArrayBuffer` detachment after growth.

7.4.5. **Long-running session tests.** Run automated sessions that simulate hours of user interaction. Monitor memory usage over time and alert on monotonic growth. Many memory leaks in Wasm applications are slow—a few kilobytes per interaction—and only become visible after hundreds or thousands of operations.

7.4.6. **Miri.** Run the test suite under Miri (`cargo +nightly miri test`) to detect undefined behavior in `unsafe` code. Miri is an interpreter for Rust's MIR (Mid-level IR) that can detect:

- Use-after-free.
- Out-of-bounds memory access.
- Invalid pointer arithmetic.
- Data races (with the `-Zmiri-check-number-validity` flag).

Miri does not target Wasm directly, but most `unsafe` bugs are independent of the compilation target and will be detected in native Miri execution.

### 7.5 Engineering Discipline

7.5.1. **Strict ownership boundaries.** Define clear ownership for every piece of data that crosses the JS/Wasm boundary. Document whether the caller or callee is responsible for freeing each buffer. Use Rust's type system to enforce ownership within the Wasm module; use documentation and code review to enforce it at the interop boundary.

7.5.2. **Memory budgeting.** Establish per-subsystem memory budgets:

- The rendering engine may use up to X MB of linear memory.
- The document model may use up to Y MB.
- Temporary buffers for image processing may use up to Z MB.

Instrument allocators to report per-subsystem usage and alert when budgets are exceeded. This transforms memory usage from an emergent, uncontrolled property into a designed, monitored constraint.

7.5.3. **Explicit lifecycle phases.** Design the application with explicit memory lifecycle phases:

- **Initialization**: Load the module, allocate long-lived data structures, establish baselines.
- **Steady state**: Process user interactions within pre-allocated budgets.
- **Peak processing**: Handle large operations (file import, export, complex computation) with temporary arena allocations that are released immediately.
- **Teardown**: Destroy the module instance if the session is ending, or reset arenas and scratch buffers to return to steady state.

7.5.4. **Interop contracts.** Define and enforce contracts for every function in the JS/Wasm API surface:

- What type and size of data does the function accept?
- Does the function allocate memory? If so, how much?
- Who owns the output buffer?
- Can the function trigger `memory.grow`?
- What is the function's behavior when memory is exhausted?

These contracts should be documented in the API definition and verified by tests.

### 7.6 Architectural Wisdom

7.6.1. **Treat Wasm memory as a bounded system resource.** Linear memory is not an infinite heap. It is a finite, grow-only resource analogous to a fixed-size buffer pool in an embedded system. Design for it accordingly:

- Never assume that `memory.grow` will succeed.
- Never assume that memory will be returned to the system after use.
- Always have a plan for what happens when memory is exhausted.

7.6.2. **Avoid unbounded workloads.** Any operation that allocates memory proportional to input size is a potential memory bomb. Impose limits on input sizes, processing batch sizes, and intermediate data structure sizes. Reject or decompose inputs that would exceed memory budgets rather than attempting to process them and hoping for the best.

7.6.3. **Design predictable memory phases.** An application whose memory usage follows a predictable pattern—stable baseline, bounded peaks, return to baseline—is far easier to operate than one whose memory usage is an unpredictable function of user behavior. Design the application's data flow to produce predictable memory phases:

- Pre-allocate working memory during initialization.
- Use arenas for transient allocations.
- Reset arenas after each operation.
- Monitor actual usage against expected phases.

7.6.4. **Build systems that tolerate linear-memory high-water marks.** Accept that the high-water mark will persist and design for it. Rather than trying to minimize the high-water mark (which is often impractical), ensure that:

- The high-water mark is bounded (not growing monotonically over the session).
- The application functions correctly at the high-water mark (no assumption that free memory is available).
- The user experience degrades gracefully (not catastrophically) as the high-water mark approaches the maximum memory limit.

7.6.5. **Consider module instance recycling.** For applications where the high-water mark inevitably grows over long sessions (e.g., design tools where users create and delete many objects), implement a module recycling strategy:

- Periodically serialize essential state.
- Destroy the current Wasm instance.
- Create a fresh instance with minimal linear memory.
- Deserialize the state into the fresh instance.

This is the Wasm equivalent of restarting a process to reclaim fragmented memory, and it is a legitimate production strategy used by multiple large-scale Wasm deployments.

---

## 8. Conclusion

### 8.1 Summary

8.1.1. Rust compiled to WebAssembly represents a significant advance in the security and performance of browser-based applications. Rust's ownership model eliminates entire classes of memory vulnerabilities at the language level. The Wasm sandbox provides isolation guarantees that no JavaScript framework can match. Together, they enable a class of browser application—large, stateful, performance-critical, security-sensitive—that was previously the exclusive domain of native desktop software.

8.1.2. However, the Wasm memory model introduces unique engineering constraints that have no direct equivalent in JavaScript or native development. The grow-only linear memory model, the interaction between the Rust heap allocator and Wasm pages, the `ArrayBuffer` detachment semantics, and the dual-heap architecture of JS/Wasm applications create a taxonomy of risks that demands careful architectural attention.

8.1.3. For applications that render UI via WebGL or WebGPU, the memory picture extends beyond Wasm linear memory to include GPU-side allocations—texture atlases, vertex buffers, and shader programs—that are invisible to standard Wasm memory monitoring and compete for physical RAM on unified-memory mobile devices (§5). The choice between immediate-mode and retained-mode rendering architectures further shapes the memory profile: immediate-mode designs naturally align with arena allocation and produce predictable, bounded memory usage, while retained-mode designs require more defensive engineering to avoid fragmentation and widget lifecycle leaks in a grow-only memory model (§6).

8.1.4. These constraints are manageable. The industry evidence demonstrates that companies including Google, Figma, Adobe, Autodesk, Cloudflare, and Shopify have successfully deployed large-scale Wasm applications by applying the mitigation strategies described in this paper: arena allocation, scratch buffers, streaming processing, memory budgeting, lifecycle management, module recycling, and rendering-architecture-aware GPU memory management.

### 8.2 Future Directions

8.2.1. Several in-progress WebAssembly proposals will address the constraints discussed in this paper:

- **Memory64.** The memory64 proposal extends Wasm linear memory addresses from 32 bits to 64 bits, raising the maximum addressable memory from 4 GB to the platform's virtual address space limit. While this does not solve the grow-only constraint, it removes the 4 GB ceiling that currently limits the largest Wasm applications.
- **GC proposal.** The Wasm GC proposal introduces garbage-collected reference types (structs, arrays) that live on the host GC heap rather than in linear memory. For languages that target Wasm GC (Kotlin, Dart, Java, OCaml), this eliminates the linear memory model entirely for managed objects. For Rust, the GC proposal is less directly applicable, but it enables better interop with GC'd host languages.
- **Component Model.** The Component Model (formerly "Interface Types") proposal defines a canonical ABI for inter-component communication, including efficient data transfer without the manual serialization currently required at the JS/Wasm boundary. This will reduce the dual-heap duplication problem.
- **Threads and shared memory.** The threads proposal (partially shipped) enables shared linear memory between Wasm instances running in separate Web Workers. This introduces shared-memory concurrency (and its attendant risks), but also enables more efficient architectures where multiple workers share a single linear memory rather than maintaining separate copies.
- **Memory control.** There have been informal discussions (though no formal proposal at the time of writing) about providing finer-grained memory control, including the ability to decommit pages within linear memory without shrinking the address space. This would allow allocators to return physical pages to the OS while preserving address-space layout—analogous to `madvise(MADV_DONTNEED)` on Linux.

8.2.2. Browser engines continue to improve their Wasm implementations. V8's TurboFan and Liftoff compilers, SpiderMonkey's Cranelift-based backend, and JavaScriptCore's BBQ and OMG tiers all produce increasingly efficient code. Browser memory management is also improving: Chrome's `PartitionAlloc` and V8's pointer compression reduce the overhead of the JavaScript side of dual-heap architectures.

8.2.3. The trajectory is clear: WebAssembly is becoming a first-class compilation target for systems software in the browser, and the tooling, specifications, and runtime implementations are converging to support the needs of large-scale production deployments. The memory constraints discussed in this paper are real and consequential, but they are engineering constraints—tractable, measurable, and addressable through the architectural discipline that systems engineers already practice. The Rust→Wasm platform does not eliminate the need for careful memory engineering; it provides a foundation on which careful memory engineering can be applied with confidence that the language and runtime will not silently undermine it.

---

*Paper prepared March 2026. WebAssembly specification references are to the W3C WebAssembly Core Specification 2.0. Browser implementation details reflect Chrome 124+, Firefox 126+, and Safari 17.4+.*
