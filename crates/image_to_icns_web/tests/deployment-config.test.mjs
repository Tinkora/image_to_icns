import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const staticRoot = new URL("../static/", import.meta.url);
const repositoryRoot = new URL("../../../", import.meta.url);

test("loads a safe deployment config before the editor module", async () => {
    const [html, config] = await Promise.all([
        readFile(new URL("index.html", staticRoot), "utf8"),
        readFile(new URL("config.js", staticRoot), "utf8"),
    ]);

    const configIndex = html.indexOf('<script src="config.js"></script>');
    const editorIndex = html.indexOf('<script type="module" src="editor.js"></script>');

    assert.notEqual(configIndex, -1);
    assert.ok(configIndex < editorIndex);
    assert.match(config, /globalThis\.__ICNS_WORKER_URL__ = null;/);
});

test("copies deployment config into the production build", async () => {
    const buildScript = await readFile(
        new URL("scripts/build_web.sh", repositoryRoot),
        "utf8",
    );

    assert.match(buildScript, /static\/config\.js.*output_dir.*config\.js/s);
});
