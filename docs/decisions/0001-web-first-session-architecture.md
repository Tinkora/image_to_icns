# ADR-0001: Use a browser-first Rust and WebAssembly architecture

## Status

Accepted

## Date

2026-08-09

## Context

Creating an icon requires visual crop adjustment and local file selection.
Requiring a native application would add installers, code signing, platform
permissions, and separate release testing without improving the core workflow.
Uploading source artwork to a conversion service would add privacy, storage,
abuse-prevention, and operating-cost obligations.

The useful common denominator is a browser that can run WebAssembly and expose
the File, Canvas, and Blob APIs.

## Decision

Ship a static browser editor backed by a Rust core compiled to WebAssembly.
Decode the source image, render the crop, encode all ICNS representations, and
verify the result in the browser. Download the result through a browser Blob.

Keep the static editor independent of all server infrastructure. Provide an
optional metadata-only Session Worker for MCP clients that need a temporary
editor link and completion state.

## Alternatives considered

### Native desktop application

Native file access is convenient, but installers, code signing, notarization,
and per-platform GUI testing are disproportionate for this focused tool.

### Server-side conversion

A thin client would be simpler, but source uploads weaken the privacy model and
create persistent operating costs. It is unnecessary because modern browsers
can run the conversion locally.

### Command-line-only interface

A CLI is easy for automation but does not provide an ergonomic visual crop
workflow. It may be added separately without replacing the browser editor.

## Consequences

- The browser release supports formats that decode consistently in WASM: PNG,
  JPEG, and SVG.
- macOS PDFKit decoding remains a native core capability and is not exposed by
  the browser editor.
- The site can be hosted as static files on GitHub Pages.
- Source images and generated files do not cross a network boundary.
- Browser behavior must be tested at supported viewport widths and with real
  file import, Canvas rendering, and download flows.
