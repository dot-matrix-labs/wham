# Technical Review: "Rust → WebAssembly Memory Constraints in Modern Browsers"

**Reviewer background.** 20+ years in systems engineering spanning browser engine internals (rendering pipelines, JS/Wasm runtimes, process architecture), virtual machine design (JIT compilers, garbage collectors, memory subsystems), and web platform infrastructure serving billions of monthly active users. Shipped memory-critical subsystems in production browsers and large-scale web applications.

**Review date.** March 2026

**Overall assessment.** The paper is a competent survey that will be useful to engineers entering the Rust/Wasm space. It correctly identifies the major categories of memory risk and provides a reasonable mitigation playbook. However, it has significant gaps in technical depth, contains several claims that are imprecise or outdated, and occasionally reads more like advocacy than analysis. Below is a section-by-section critique.

---

## 1. Section 1 — Motivation

### 1.1 Overstated security framing

1.1.1. The paper frames Rust→Wasm almost exclusively as a security decision. This is misleading. In every large-scale deployment I have been involved with or have direct knowledge of (Figma, Adobe, Google Earth), the primary driver was **performance**, not security. Security is a welcome side effect, but no VP of Engineering signed off on a multi-year Wasm migration because of prototype pollution. The motivation section should lead with performance and determinism, then discuss security as a reinforcing benefit.

1.1.2. The list of C/C++ vulnerabilities (§1.1.2) is accurate but contextually misleading. It implies that Rust→Wasm replaces native plugins (NPAPI, ActiveX), but those architectures have been dead for nearly a decade. The actual comparison point in 2026 is JavaScript — and Rust→Wasm does not replace the browser engine's own C++ code, which is where most of those CVEs originate. The paper conflates the security of *application code* (which the developer controls) with the security of *the runtime* (which they do not).

### 1.2 Missing nuance on Rust safety

1.2.1. The claim that Rust "eliminates" use-after-free, data races, etc. (§1.1.3) needs qualification. It eliminates them **in safe Rust**. Any non-trivial Wasm application uses `unsafe` — for FFI bindings (`wasm-bindgen` generates `unsafe` code), for performance-critical inner loops, for interfacing with allocators. The paper acknowledges this in §4.1 but the motivating section creates an impression of absolute safety that §4.1 then contradicts. A reader who only skims Section 1 will come away with a false sense of security.

1.2.2. The paper does not mention `unsafe` statistics. In my experience reviewing production Wasm codebases, the ratio of `unsafe` blocks to total code is higher in Wasm targets than in typical server-side Rust, precisely because of the FFI-heavy interop layer. This undermines the security narrative.

### 1.3 JavaScript attack surface: partially stale

1.3.1. The supply-chain comparison (§1.2.1) is valid but the claim that Rust dependencies are "auditable at the source level" is naive. `crates.io` allows build scripts (`build.rs`) that execute arbitrary code at compile time. `cargo-audit` exists but adoption is inconsistent. The 2024 `xz`-style attacks showed that source-level auditability does not prevent sophisticated supply-chain compromise. The paper should acknowledge that Rust's supply chain is *different*, not *solved*.

1.3.2. The claim that Wasm linear memory provides "a degree of side-channel resistance" (§1.2.4) is wrong. Spectre v1 gadgets can be constructed within Wasm linear memory itself — this was demonstrated by the original Spectre paper (Kocher et al., 2018) and is the reason browsers disabled `SharedArrayBuffer` and high-resolution timers. Wasm isolation provides *sandboxing*, not side-channel resistance. These are different properties.

---

## 2. Section 2 — Industry Adoption

### 2.1 Factual accuracy

2.1.1. The Figma characterization is correct but incomplete. Figma's engine is C++, not Rust. The paper's title and framing are "Rust → WebAssembly" but the most prominent example is a C++ → Wasm deployment. This is a significant gap. The paper should either broaden its scope to "compiled languages → Wasm" or provide Rust-specific examples for each deployment category. As written, it implicitly credits Rust's safety model for deployments that do not use Rust.

2.1.2. The Google Sheets claim (§2.3.1) — "Google Sheets uses Wasm for computational kernels" — needs a citation. I am not aware of public confirmation of this. Unverifiable claims undermine the paper's credibility with the stated audience (systems engineers and security researchers who will check).

2.1.3. The Autodesk section (§2.1.3) is vague. "Portions of their Fusion and AutoCAD web clients" — which portions? What language? What scale? Without specifics, this is padding.

### 2.2 Missing examples

2.2.1. The paper omits several significant Rust→Wasm deployments that would strengthen the evidence section:

- **1Password** uses Rust compiled to Wasm for its browser extension's cryptographic core.
- **Discord** uses Rust (compiled natively, not to Wasm, but relevant to the Rust ecosystem discussion).
- **Amazon Prime Video** has discussed Wasm use for client-side media processing.
- **Zaplib** (defunct but instructive) attempted a full Rust→Wasm UI framework and published detailed postmortems about the memory challenges — directly relevant to this paper.

2.2.2. The paper lacks any discussion of **failures**. Survivorship bias is a serious problem in technology survey papers. Zaplib's shutdown, the performance regressions Figma encountered during their asm.js→Wasm migration, and the memory issues that have forced multiple teams to implement worker recycling (§5.3.5) are all important data points that a balanced paper should include.

---

## 3. Section 3 — Memory Model

### 3.1 Technically solid but incomplete

3.1.1. This is the strongest section of the paper. The explanation of linear memory, page-based growth, and `ArrayBuffer` detachment is accurate and well-structured.

3.1.2. However, the section omits several important details:

- **Virtual memory overcommit.** On Linux (the dominant server-side Wasm platform and a significant desktop platform), V8's 4 GB virtual reservation interacts with the kernel's overcommit settings. In containers with strict memory limits (Kubernetes, cgroups v2), the OOM killer may terminate the process when committed Wasm pages exceed the cgroup's memory limit, even if the application's live data is small. This is a production-critical issue that the paper does not mention.

- **Memory mapping strategies.** The paper describes V8's virtual memory reservation but does not discuss the implications for 32-bit platforms, embedded WebViews (Android WebView, WKWebView), or environments where virtual address space is constrained. These are significant deployment targets.

- **`memory.grow` performance.** The paper says `memory.grow` is "relatively cheap" (§3.5.1). This is true for small growths on 64-bit platforms but misleading in general. On some platforms, `memory.grow` triggers a page-fault storm as the OS lazily commits physical pages. On others, it requires copying the entire linear memory (older 32-bit implementations). The performance characteristics vary by an order of magnitude across engines and platforms.

### 3.2 Missing: multi-memory proposal

3.2.1. The multi-memory proposal (Phase 4, near standardization) allows a Wasm module to declare multiple independent linear memories. This is directly relevant to the paper's concerns: an application can use one memory for long-lived data and another for temporary allocations, destroying and recreating the temporary memory to reclaim pages. The paper's §5 mitigation strategies would benefit significantly from discussing multi-memory as an architectural option.

---

## 4. Section 4 — Taxonomy of Memory Risks

### 4.1 Good structure, insufficient depth

4.1.1. The three-category taxonomy (Rust-exclusive, Wasm-specific, amplified cross-boundary) is well-chosen and useful as an organizing framework.

4.1.2. However, the individual risk descriptions are often too abstract. A paper targeting systems engineers should include:

- **Concrete failure scenarios.** Not "allocator corruption can cause subsequent allocations to return overlapping regions" but a specific, reproducible sequence: a double-free in `unsafe` code corrupts dlmalloc's bin metadata, causing the next two `malloc` calls to return the same address, leading to data corruption when two subsystems write to the "same" allocation.

- **Prevalence data.** How common are these bugs in practice? The paper treats all risks as equally likely, but in my experience, stale typed-array views (§4.2.2) account for more production incidents than all of §4.1 combined. Prioritization matters.

- **Detection difficulty.** Some of these bugs produce immediate traps (out-of-bounds access). Others produce silent corruption that manifests hours later. The paper should classify each risk by detectability, not just severity.

### 4.2 Missing risks

4.2.1. **OOM behavior.** The paper does not adequately discuss what happens when `memory.grow` returns -1 (failure). In Rust, the global allocator's `alloc` method returns a null pointer, which Rust's allocation infrastructure converts to an `alloc::alloc::handle_alloc_error` call, which by default aborts. In Wasm, "abort" means a trap, which means the entire module instance is terminated. There is no recovery. This is a critical architectural constraint: a single allocation failure anywhere in the program — including in third-party library code — is fatal. The paper mentions "never assume that `memory.grow` will succeed" (§5.6.1) but does not explain the catastrophic failure mode that results from failure.

4.2.2. **WASI and non-browser Wasm runtimes.** The paper's title says "Modern Browsers" but the mitigation strategies (§5) apply equally to WASI runtimes (Wasmtime, Wasmer, WasmEdge). The memory model is the same, but the resource limits, virtual memory behavior, and failure modes differ. Acknowledging this scope boundary would strengthen the paper.

4.2.3. **Thread safety of `memory.grow`.** With the threads proposal, `memory.grow` on a shared memory is atomic with respect to other threads' memory accesses, but the JavaScript-side `ArrayBuffer` detachment is not atomic with respect to JavaScript code on other threads reading from the old buffer. This is a subtle concurrency hazard that the paper does not discuss.

4.2.4. **Code-size-induced memory pressure.** Large Wasm binaries (10+ MB for complex Rust applications) consume memory for compiled code, not just linear memory. On memory-constrained mobile devices, the compiled code's memory footprint can rival the linear memory footprint. The paper focuses exclusively on linear memory and ignores code memory.

---

## 5. Section 5 — Mitigation Strategies

### 5.1 Reasonable but undersells difficulty

5.1.1. The "easiest to hardest" ordering is useful. The tooling recommendations (§5.1) are accurate and actionable.

5.1.2. However, the architecture patterns (§5.3) undersell the implementation difficulty:

- **Arena allocation** (§5.3.1) is presented as a silver bullet. It is not. Arena allocation works well for request/response workloads but poorly for long-lived, mutating data structures (e.g., a document model that supports undo/redo). The paper does not discuss when arenas are inappropriate.

- **Worker lifecycle resets** (§5.3.5) require serializing and deserializing the entire application state. For a complex application (design tool, code editor), this serialization can take seconds and may itself require temporary memory that exceeds the budget. The paper mentions this adds "complexity" but does not quantify the engineering cost, which in my experience is measured in engineer-months for a non-trivial application.

- **Streaming processing** (§5.3.3) is not always possible. Many algorithms require random access to their input (sorting, spatial indexing, graph algorithms). The paper should acknowledge the class of problems where streaming is inapplicable.

### 5.2 Missing mitigations

5.2.1. **`wasm-bindgen` weak references.** The `wasm-bindgen` crate supports JavaScript ` weak-ref` and `FinalizationRegistry` for preventing the callback lifetime leaks described in §4.3.3. This is the single most impactful mitigation for the most common class of Wasm memory leak, and the paper does not mention it.

5.2.2. **Memory pressure API.** The `navigator.deviceMemory` API and the experimental `performance.measureUserAgentSpecificMemory()` API allow applications to adapt their memory strategy to the device's capabilities. A design tool on a 4 GB mobile device should use different memory budgets than the same tool on a 64 GB workstation. The paper mentions `performance.measureUserAgentSpecificMemory()` for monitoring but not for adaptive budgeting.

5.2.3. **Compile-time memory layout optimization.** Rust's `#[repr(C)]`, `#[repr(packed)]`, and `#[repr(align)]` attributes control struct layout. In Wasm applications where memory is at a premium, careful struct layout (field ordering to minimize padding, using smaller integer types) can reduce memory usage by 10–30% for data-heavy applications. This is a zero-runtime-cost mitigation that the paper omits.

5.2.4. **Wasm-specific allocator tuning.** The paper mentions allocator selection (§5.2.2) but does not discuss tuning. dlmalloc's `MORECORE` granularity, free-list bin sizes, and mmap threshold equivalents are all configurable and have significant impact on fragmentation behavior. For production Wasm applications, allocator tuning is not optional.

---

## 6. Section 6 — Conclusion

### 6.1 Future directions

6.1.1. The discussion of future proposals (§6.2) is reasonable but misses the most impactful near-term change: **multi-memory** (discussed in §3.2 of this review). Multi-memory is further along in the standardization process than several of the proposals mentioned and directly addresses the paper's core concerns.

6.1.2. The paper does not discuss **Wasm GC's implications for Rust**. While the GC proposal primarily benefits managed languages, it also enables Rust→Wasm applications to store interop objects on the GC heap rather than in linear memory, reducing the dual-heap problem. The `wasm-bindgen` team has discussed leveraging Wasm GC for reference type management.

6.1.3. The "memory control" discussion (§6.2.1, final bullet) correctly identifies `madvise(MADV_DONTNEED)` semantics as the most impactful missing capability. This deserves more emphasis — it is arguably more important than memory64 for production deployments, because the 4 GB limit is rarely the binding constraint while the inability to decommit pages is *always* a constraint.

---

## 7. Structural and Stylistic Issues

### 7.1 Advocacy vs. analysis

7.1.1. The paper oscillates between two modes: technical analysis (Sections 3–5) and advocacy for Rust→Wasm adoption (Sections 1–2, parts of Section 6). The advocacy sections weaken the paper's credibility with the stated audience. Systems engineers and security researchers do not need to be convinced that Wasm exists; they need accurate risk analysis. Recommendation: cut Section 1 to one page of factual context, move the security discussion to an appendix, and let the technical content speak for itself.

### 7.2 Missing quantitative data

7.2.1. The paper contains no benchmarks, no measurements, no graphs, and no quantitative data of any kind. For a paper about memory constraints, this is a significant omission. At minimum, the paper should include:

- Memory growth curves for representative workloads (e.g., "editing a 500-object Figma document for 4 hours").
- Fragmentation ratios (live data vs. committed pages) for different allocator strategies.
- Cross-boundary copy overhead measurements for common data types and sizes.
- `memory.grow` latency measurements across engines and platforms.

### 7.3 Citation gaps

7.3.1. The paper makes numerous factual claims about browser internals, specification status, and company deployments without citations. An academic-style paper should cite:

- The WebAssembly specification for all specification claims.
- Published engineering blog posts for industry deployment claims.
- CVE databases for vulnerability prevalence claims.
- The original Spectre paper for side-channel claims.

### 7.4 Legal-style numbering

7.4.1. The numbered-paragraph format was requested and is consistently applied. However, some paragraphs (e.g., §1.1.2's long bullet list) would be more readable as a numbered sub-list within the paragraph rather than inline bullets. This is a minor formatting note.

---

## 8. Summary Verdict

| Dimension | Rating | Notes |
|---|---|---|
| Technical accuracy | 7/10 | Mostly correct; Spectre claim is wrong; some claims unverifiable |
| Completeness | 6/10 | Missing multi-memory, OOM behavior, quantitative data, failure case studies |
| Depth | 5/10 | Stays at survey level; lacks the concrete failure scenarios that systems engineers need |
| Audience fit | 6/10 | Too much advocacy for the stated audience; needs more data, fewer adjectives |
| Practical utility | 7/10 | Mitigation playbook is genuinely useful; tooling recommendations are actionable |
| Novelty | 4/10 | Synthesizes known information; does not contribute new analysis or measurements |

**Overall: 6/10.** A solid first draft that would benefit from (1) cutting the advocacy framing, (2) adding quantitative measurements, (3) discussing multi-memory and OOM failure modes, (4) including failure case studies alongside success stories, and (5) adding citations throughout. In its current form, it is a useful onboarding document for engineers new to Wasm memory management but falls short of the "technically rigorous" standard claimed in its preamble.

---

## 9. Addendum: Revision Notes (March 2026)

The paper has been revised to address two critical gaps identified in this review and in subsequent analysis:

### 9.1 GPU Memory Constraints (new §5)

The paper now includes a dedicated section on GPU-side memory constraints for applications that render via WebGL/WebGPU. This section covers: the three-heap memory model (Wasm linear memory + JS heap + GPU memory), texture atlas memory accounting, glyph cache growth in form-heavy applications, GPU context loss and its architectural implications, and GPU memory budgeting for mobile devices. This was a significant omission given that the paper's primary audience includes engineers building GPU-rendered Wasm applications.

### 9.2 Rendering Architecture Analysis (new §6)

The paper now analyzes how the choice between immediate-mode and retained-mode rendering architectures shapes the memory profile of a Wasm UI application. This includes: memory characteristics of each approach (widget tree persistence, fragmentation risk, arena suitability), risks specific to each mode (GPU buffer churn for immediate, widget lifecycle leaks for retained), hybrid approaches (Xilem, Makepad), and a concrete recommendation for the forms-focused use case. The recommendation—that immediate-mode or immediate-mode-hybrid architectures are strongly favored for memory reasons—is well-supported by the analysis and directly addresses the paper's central concern about fragmentation in a grow-only memory model.

### 9.3 Revised assessment

With these additions, the paper's completeness improves materially. The rendering architecture section in particular fills what was arguably the largest analytical gap: the paper previously discussed memory constraints in the abstract without connecting them to the architectural decisions that determine whether those constraints are manageable or catastrophic. The GPU memory section closes the gap between "Wasm linear memory" analysis and the reality that GPU-rendered applications have a second, often larger memory domain to manage.

The remaining gaps from the original review (quantitative data, multi-memory proposal, OOM failure modes, failure case studies, citations) are still unaddressed and still matter.
