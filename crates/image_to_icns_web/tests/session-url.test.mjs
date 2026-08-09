import assert from "node:assert/strict";
import test from "node:test";

import { readSessionParams } from "../static/session-url.mjs";

test("reads session fragment and removes it from browser history", () => {
    const location = {
        hash: "#session=session-123&secret=top-secret&worker=https%3A%2F%2Fworker.example.com",
        pathname: "/editor/",
        search: "?locale=en",
    };
    const replacements = [];
    const history = {
        state: { navigation: "test" },
        replaceState(state, title, url) {
            replacements.push({ state, title, url });
        },
    };

    const result = readSessionParams(location, history, null);

    assert.deepEqual(result, {
        sessionId: "session-123",
        sessionSecret: "top-secret",
        workerBaseUrl: "https://worker.example.com",
    });
    assert.deepEqual(replacements, [
        {
            state: { navigation: "test" },
            title: "",
            url: "/editor/?locale=en",
        },
    ]);
});

test("removes a sensitive fragment before rejecting an invalid Worker URL", () => {
    const location = {
        hash: "#session=session-123&secret=top-secret&worker=http%3A%2F%2Fexample.com",
        pathname: "/editor/",
        search: "",
    };
    const replacements = [];
    const history = {
        state: null,
        replaceState(state, title, url) {
            replacements.push({ state, title, url });
        },
    };

    assert.throws(
        () => readSessionParams(location, history, null),
        /Worker URL must use HTTPS/,
    );
    assert.deepEqual(replacements, [
        { state: null, title: "", url: "/editor/" },
    ]);
});

test("removes legacy query credentials while preserving unrelated parameters", () => {
    const location = {
        hash: "",
        pathname: "/editor/",
        search: "?locale=en&session=session-123&secret=top-secret&worker=https%3A%2F%2Fworker.example.com",
    };
    const replacements = [];
    const history = {
        state: null,
        replaceState(state, title, url) {
            replacements.push({ state, title, url });
        },
    };

    const result = readSessionParams(location, history, null);

    assert.deepEqual(result, {
        sessionId: "session-123",
        sessionSecret: "top-secret",
        workerBaseUrl: "https://worker.example.com",
    });
    assert.deepEqual(replacements, [
        { state: null, title: "", url: "/editor/?locale=en" },
    ]);
});

test("cleans legacy query credentials without removing an unrelated fragment", () => {
    const location = {
        hash: "#help",
        pathname: "/editor/",
        search: "?locale=en&session=session-123&secret=top-secret&worker=https%3A%2F%2Fworker.example.com",
    };
    const replacements = [];
    const history = {
        state: null,
        replaceState(state, title, url) {
            replacements.push({ state, title, url });
        },
    };

    const result = readSessionParams(location, history, null);

    assert.deepEqual(result, {
        sessionId: "session-123",
        sessionSecret: "top-secret",
        workerBaseUrl: "https://worker.example.com",
    });
    assert.deepEqual(replacements, [
        { state: null, title: "", url: "/editor/?locale=en#help" },
    ]);
});
