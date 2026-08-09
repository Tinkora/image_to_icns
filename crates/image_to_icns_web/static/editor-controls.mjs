const KEYBOARD_PAN_STEP = 12;

/** Map an arrow key to image movement in canvas pixels. */
export function keyboardPanDelta(key) {
    switch (key) {
        case "ArrowLeft":
            return { x: -KEYBOARD_PAN_STEP, y: 0 };
        case "ArrowRight":
            return { x: KEYBOARD_PAN_STEP, y: 0 };
        case "ArrowUp":
            return { x: 0, y: -KEYBOARD_PAN_STEP };
        case "ArrowDown":
            return { x: 0, y: KEYBOARD_PAN_STEP };
        default:
            return null;
    }
}

/** Convert image movement on the canvas to normalized crop coordinates. */
export function centerAfterCanvasPan({
    centerX,
    centerY,
    sourceWidth,
    sourceHeight,
    zoom,
    canvasWidth,
    canvasHeight,
    deltaX,
    deltaY,
}) {
    const shortSide = Math.min(sourceWidth, sourceHeight);
    return {
        x: centerX - (deltaX / canvasWidth) * (shortSide / sourceWidth) / zoom,
        y: centerY - (deltaY / canvasHeight) * (shortSide / sourceHeight) / zoom,
    };
}
