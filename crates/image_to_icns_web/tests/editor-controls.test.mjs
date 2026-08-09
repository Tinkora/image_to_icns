import assert from "node:assert/strict";
import test from "node:test";

import {
    centerAfterCanvasPan,
    keyboardPanDelta,
} from "../static/editor-controls.mjs";

test("maps arrow keys to stable image movement", () => {
    assert.deepEqual(keyboardPanDelta("ArrowLeft"), { x: -12, y: 0 });
    assert.deepEqual(keyboardPanDelta("ArrowRight"), { x: 12, y: 0 });
    assert.deepEqual(keyboardPanDelta("ArrowUp"), { x: 0, y: -12 });
    assert.deepEqual(keyboardPanDelta("ArrowDown"), { x: 0, y: 12 });
    assert.equal(keyboardPanDelta("Enter"), null);
});

test("converts canvas movement to normalized crop center movement", () => {
    const center = centerAfterCanvasPan({
        centerX: 0.5,
        centerY: 0.5,
        sourceWidth: 800,
        sourceHeight: 400,
        zoom: 2,
        canvasWidth: 400,
        canvasHeight: 400,
        deltaX: 40,
        deltaY: -20,
    });

    assert.deepEqual(center, { x: 0.475, y: 0.525 });
});
