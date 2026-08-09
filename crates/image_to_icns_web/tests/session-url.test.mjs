import assert from "node:assert/strict";
import test from "node:test";

import {
    buildSessionEndpoint,
    readSessionParams,
} from "../static/session-url.mjs";

const VALID_SESSION_ID = "a".repeat(64);
const VALID_SESSION_SECRET = "b".repeat(128);

test("reads session fragment and removes it from browser history", () => {
    const location = {
        hash: `#session=${VALID_SESSION_ID}&secret=${VALID_SESSION_SECRET}&worker=https%3A%2F%2Fworker.example.com`,
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

    const result = readSessionParams(
        location,
        history,
        "https://worker.example.com/",
    );

    assert.deepEqual(result, {
        sessionId: VALID_SESSION_ID,
        sessionSecret: VALID_SESSION_SECRET,
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
        hash: `#session=${VALID_SESSION_ID}&secret=${VALID_SESSION_SECRET}&worker=http%3A%2F%2Fexample.com`,
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
        () => readSessionParams(location, history, "https://worker.example.com"),
        /Worker URL must use HTTPS/,
    );
    assert.deepEqual(replacements, [
        { state: null, title: "", url: "/editor/" },
    ]);
});

test("removes and rejects query credentials while preserving unrelated parameters", () => {
    const location = {
        hash: "",
        pathname: "/editor/",
        search: `?locale=en&session=${VALID_SESSION_ID}&secret=${VALID_SESSION_SECRET}&worker=https%3A%2F%2Fworker.example.com`,
    };
    const replacements = [];
    const history = {
        state: null,
        replaceState(state, title, url) {
            replacements.push({ state, title, url });
        },
    };

    assert.throws(
        () => readSessionParams(location, history, "https://worker.example.com"),
        /Session credentials must use the URL fragment/,
    );
    assert.deepEqual(replacements, [
        { state: null, title: "", url: "/editor/?locale=en" },
    ]);
});

test("rejects query credentials without removing an unrelated fragment", () => {
    const location = {
        hash: "#help",
        pathname: "/editor/",
        search: `?locale=en&session=${VALID_SESSION_ID}&secret=${VALID_SESSION_SECRET}&worker=https%3A%2F%2Fworker.example.com`,
    };
    const replacements = [];
    const history = {
        state: null,
        replaceState(state, title, url) {
            replacements.push({ state, title, url });
        },
    };

    assert.throws(
        () => readSessionParams(location, history, "https://worker.example.com"),
        /Session credentials must use the URL fragment/,
    );
    assert.deepEqual(replacements, [
        { state: null, title: "", url: "/editor/?locale=en#help" },
    ]);
});

test("rejects a Session link that selects an unconfigured Worker origin", () => {
    const location = {
        hash: `#session=${VALID_SESSION_ID}&secret=${VALID_SESSION_SECRET}&worker=https%3A%2F%2Fworker.example.com`,
        pathname: "/editor/",
        search: "",
    };
    const history = {
        state: null,
        replaceState() {},
    };

    assert.throws(
        () => readSessionParams(location, history, null),
        /configured Worker URL/,
    );
});

test("rejects a Session link whose Worker origin differs from deployment config", () => {
    const location = {
        hash: `#session=${VALID_SESSION_ID}&secret=${VALID_SESSION_SECRET}&worker=https%3A%2F%2Fevil.example.com`,
        pathname: "/editor/",
        search: "",
    };
    const history = {
        state: null,
        replaceState() {},
    };

    assert.throws(
        () => readSessionParams(location, history, "https://worker.example.com"),
        /does not match the configured Worker URL/,
    );
});

test("rejects a Session ID that could alter the Worker request path", () => {
    const location = {
        hash: `#session=..%2Fadmin&secret=${VALID_SESSION_SECRET}`,
        pathname: "/editor/",
        search: "",
    };
    const history = {
        state: null,
        replaceState() {},
    };

    assert.throws(
        () => readSessionParams(location, history, "https://worker.example.com"),
        /Session ID must be 64 lowercase hexadecimal characters/,
    );
});

test("rejects a malformed Session secret", () => {
    const location = {
        hash: `#session=${VALID_SESSION_ID}&secret=not-a-secret`,
        pathname: "/editor/",
        search: "",
    };
    const history = {
        state: null,
        replaceState() {},
    };

    assert.throws(
        () => readSessionParams(location, history, "https://worker.example.com"),
        /Session secret must be 128 lowercase hexadecimal characters/,
    );
});

test("builds a Session endpoint from a configured origin and validated ID", () => {
    assert.equal(
        buildSessionEndpoint("https://worker.example.com/", VALID_SESSION_ID),
        `https://worker.example.com/sessions/${VALID_SESSION_ID}`,
    );
    assert.throws(
        () => buildSessionEndpoint("https://worker.example.com", "../admin"),
        /Session ID must be 64 lowercase hexadecimal characters/,
    );
    assert.throws(
        () => buildSessionEndpoint("https://worker.example.com/api", VALID_SESSION_ID),
        /Worker URL must be an origin without a path/,
    );
});
