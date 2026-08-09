// image_to_icns Web editor frontend logic
//
// Depends on wasm-pack build artifacts (pkg/ directory). Source images and
// generated ICNS bytes stay in browser memory; optional Session mode sends
// only secret-authenticated state metadata.

import init, { Editor, import_file } from "./pkg/image_to_icns_web.js";
import {
    centerAfterCanvasPan,
    keyboardPanDelta,
} from "./editor-controls.mjs";
import { readSessionParams } from "./session-url.mjs";

// ── State ────────────────────────────────────────────────
/** @type {Editor|null} */
let editor = null;

// ── Session parameters (read from URL) ───────────────────
/** @type {string|null} */
let sessionId = null;
/** @type {string|null} */
let sessionSecret = null;
/** @type {string|null} */
let workerBaseUrl = null;

// ── DOM References ───────────────────────────────────────
const dropZone = document.getElementById("drop-zone");
const fileInput = document.getElementById("file-input");
const importError = document.getElementById("import-error");
const stepImport = document.getElementById("step-import");
const editorWorkspace = document.getElementById("editor-workspace");
const cropTitle = document.getElementById("crop-title");
const replaceBtn = document.getElementById("replace-btn");
const previewCanvas = document.getElementById("preview-canvas");
const zoomSlider = document.getElementById("zoom-slider");
const zoomValue = document.getElementById("zoom-value");
const sourceDims = document.getElementById("source-dims");
const generateBtn = document.getElementById("generate-btn");
const downloadBtn = document.getElementById("download-btn");
const exportStatus = document.getElementById("export-status");

// ── Canvas drag state ────────────────────────────────────
let dragging = false;
let lastPointerX = 0;
let lastPointerY = 0;

// ── Initialization ───────────────────────────────────────
async function main() {
    // 1. Parse Session parameters from URL
    try {
        parseSessionParams();
    } catch (err) {
        showError(importError, `Invalid Session link: ${err}`);
        return;
    }

    // 2. Load WASM
    try {
        await init();
        console.log("WASM module loaded");
    } catch (err) {
        showError(importError, `WASM load failed: ${err}`);
        return;
    }
    bindEvents();

    // 3. If Session exists, mark state as editing
    if (sessionId && sessionSecret) {
        patchSessionState("editing").catch((err) => {
            console.warn("Failed to update Session state to editing:", err);
        });
    }
}

// ── Event bindings ───────────────────────────────────────
function bindEvents() {
    // Click to select file
    dropZone.addEventListener("click", () => openFilePicker());
    fileInput.addEventListener("change", handleFileSelect);
    replaceBtn.addEventListener("click", () => openFilePicker());

    // Drag and drop
    dropZone.addEventListener("dragover", (e) => {
        e.preventDefault();
        dropZone.classList.add("drag-over");
    });
    dropZone.addEventListener("dragleave", () => {
        dropZone.classList.remove("drag-over");
    });
    dropZone.addEventListener("drop", (e) => {
        e.preventDefault();
        dropZone.classList.remove("drag-over");
        const files = e.dataTransfer?.files;
        if (files?.length) processFile(files[0]);
    });

    // Zoom slider
    zoomSlider.addEventListener("input", () => {
        if (!editor) return;
        const z = parseFloat(zoomSlider.value);
        zoomValue.textContent = `${z.toFixed(2)}x`;
        editor.set_zoom(z);
        renderPreview();
    });

    // Canvas drag
    previewCanvas.addEventListener("pointerdown", (e) => {
        dragging = true;
        lastPointerX = e.clientX;
        lastPointerY = e.clientY;
        previewCanvas.setPointerCapture(e.pointerId);
    });
    previewCanvas.addEventListener("pointermove", (e) => {
        if (!dragging || !editor) return;
        const dx = e.clientX - lastPointerX;
        const dy = e.clientY - lastPointerY;
        lastPointerX = e.clientX;
        lastPointerY = e.clientY;

        panPreview(dx, dy);
    });
    previewCanvas.addEventListener("pointerup", () => { dragging = false; });
    previewCanvas.addEventListener("pointercancel", () => { dragging = false; });
    previewCanvas.addEventListener("keydown", (event) => {
        const delta = keyboardPanDelta(event.key);
        if (!delta || !editor) return;
        event.preventDefault();
        panPreview(delta.x, delta.y);
    });

    // Generate
    generateBtn.addEventListener("click", async () => {
        if (!editor) return;
        try {
            generateBtn.disabled = true;
            generateBtn.setAttribute("aria-busy", "true");
            exportStatus.className = "";
            exportStatus.textContent = "Generating ICNS...";
            downloadBtn.classList.add("hidden");

            // Generate ICNS data
            const icnsBytes = editor.generate_icns();
            exportStatus.className = "success";
            exportStatus.textContent = `ICNS ready - ${(icnsBytes.length / 1024).toFixed(1)} KB`;
            downloadBtn.classList.remove("hidden");

            // Bind download (with Session callback)
            downloadBtn.onclick = () => {
                downloadIcns(icnsBytes, "icon.icns");
                // Report to Worker asynchronously after download
                if (sessionId && sessionSecret) {
                    patchSessionState("completed", {
                        output_byte_len: icnsBytes.length,
                        representation_count: 10,
                    }).catch((err) => {
                        console.warn("Failed to report Session completion:", err);
                    });
                }
            };
        } catch (err) {
            exportStatus.className = "error";
            exportStatus.textContent = `Generation failed: ${err}`;
        } finally {
            generateBtn.disabled = false;
            generateBtn.removeAttribute("aria-busy");
        }
    });
}

// ── File handling ───────────────────────────────────────
function handleFileSelect(e) {
    const file = e.target.files?.[0];
    if (file) processFile(file);
}

function openFilePicker() {
    fileInput.value = "";
    fileInput.click();
}

async function processFile(file) {
    hideError(importError);
    try {
        const image = await import_file(file);
        const sourceWidth = image.width;
        const sourceHeight = image.height;
        editor?.free();
        editor = new Editor(image);

        // Update UI
        sourceDims.textContent = `${sourceWidth} x ${sourceHeight} px`;
        stepImport.classList.add("hidden");
        editorWorkspace.classList.remove("hidden");

        // Reset crop parameters
        zoomSlider.value = 1;
        zoomValue.textContent = "1.00x";
        exportStatus.className = "message hidden";
        exportStatus.textContent = "";
        downloadBtn.classList.add("hidden");

        renderPreview();
        cropTitle.focus();
    } catch (err) {
        showError(importError, `Import failed: ${err}`);
    }
}

// ── Preview rendering ───────────────────────────────────
function renderPreview() {
    if (!editor) return;
    try {
        editor.preview(previewCanvas);
    } catch (err) {
        console.error("Preview render failed:", err);
    }
}

function panPreview(deltaX, deltaY) {
    if (!editor) return;
    const bounds = previewCanvas.getBoundingClientRect();
    const center = centerAfterCanvasPan({
        centerX: editor.center_x(),
        centerY: editor.center_y(),
        sourceWidth: editor.source_width,
        sourceHeight: editor.source_height,
        zoom: editor.zoom(),
        canvasWidth: bounds.width,
        canvasHeight: bounds.height,
        deltaX,
        deltaY,
    });
    editor.set_center(center.x, center.y);
    renderPreview();
}

// ── Utilities ────────────────────────────────────────────
function showError(el, msg) {
    el.textContent = msg;
    el.classList.remove("hidden");
}

function hideError(el) {
    el.classList.add("hidden");
}

function downloadIcns(bytes, filename) {
    const blob = new Blob([bytes], { type: "image/icns" });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = filename;
    anchor.hidden = true;
    document.body.appendChild(anchor);
    anchor.click();
    anchor.remove();
    setTimeout(() => URL.revokeObjectURL(url), 0);
}

// ── Session utilities ────────────────────────────────────

/** Parse session and secret parameters from URL. */
function parseSessionParams() {
    const params = readSessionParams(
        window.location,
        window.history,
        window.__ICNS_WORKER_URL__,
    );
    sessionId = params.sessionId;
    sessionSecret = params.sessionSecret;
    workerBaseUrl = params.workerBaseUrl;
}

/**
 * Report Session state change to Worker.
 * @param {string} state - Target state (editing / completed / failed)
 * @param {object} [extra] - Extra fields
 */
async function patchSessionState(state, extra = {}) {
    if (!workerBaseUrl) {
        throw new Error("Worker URL not configured");
    }
    const body = {
        state,
        secret: sessionSecret,
        ...extra,
    };
    const resp = await fetch(`${workerBaseUrl}/sessions/${sessionId}`, {
        method: "PATCH",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body),
    });
    if (!resp.ok) {
        const text = await resp.text();
        throw new Error(`Worker returned ${resp.status}: ${text}`);
    }
    console.log("Session state updated:", state);
    return resp.json();
}

// ── Startup ──────────────────────────────────────────────
main();
