#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
web_crate="${repo_root}/crates/image_to_icns_web"
output_dir="${repo_root}/dist"

wasm-pack build "${web_crate}" --target web --release --locked

rm -rf "${output_dir}"
mkdir -p "${output_dir}/pkg"
cp "${web_crate}/static/index.html" "${output_dir}/index.html"
cp "${web_crate}/static/editor.js" "${output_dir}/editor.js"
cp "${web_crate}/static/editor-controls.mjs" "${output_dir}/editor-controls.mjs"
cp "${web_crate}/static/session-url.mjs" "${output_dir}/session-url.mjs"
cp "${web_crate}/static/style.css" "${output_dir}/style.css"
cp "${web_crate}/static/favicon.svg" "${output_dir}/favicon.svg"
cp "${web_crate}/pkg/"* "${output_dir}/pkg/"
touch "${output_dir}/.nojekyll"
