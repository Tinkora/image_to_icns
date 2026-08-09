/** Read session parameters and remove sensitive fragments from browser history. */
export function readSessionParams(location, history, configuredWorker) {
    const fragmentParams = new URLSearchParams(location.hash.slice(1));
    const queryParams = new URLSearchParams(location.search);
    const fragmentContainsSessionState = containsSessionState(fragmentParams);
    const queryContainsSessionState = containsSessionState(queryParams);

    if (fragmentContainsSessionState || queryContainsSessionState) {
        queryParams.delete("session");
        queryParams.delete("secret");
        queryParams.delete("worker");
        const remainingQuery = queryParams.toString();
        const remainingFragment = fragmentContainsSessionState ? "" : location.hash;
        const cleanUrl = `${location.pathname}${remainingQuery ? `?${remainingQuery}` : ""}${remainingFragment}`;
        history.replaceState(history.state, "", cleanUrl);
    }

    if (queryContainsSessionState) {
        throw new Error("Session credentials must use the URL fragment");
    }

    const sessionId = fragmentParams.get("session");
    const sessionSecret = fragmentParams.get("secret");
    const requestedWorker = fragmentParams.get("worker");

    const workerBaseUrl = configuredWorker
        ? normalizeWorkerBaseUrl(configuredWorker)
        : null;
    const requestedWorkerBaseUrl = requestedWorker
        ? normalizeWorkerBaseUrl(requestedWorker)
        : null;

    if (requestedWorkerBaseUrl && !workerBaseUrl) {
        throw new Error("Session mode requires a configured Worker URL");
    }
    if (requestedWorkerBaseUrl && requestedWorkerBaseUrl !== workerBaseUrl) {
        throw new Error("Session link Worker URL does not match the configured Worker URL");
    }
    if (sessionId !== null && !isValidSessionId(sessionId)) {
        throw new Error("Session ID must be 64 lowercase hexadecimal characters");
    }
    if (sessionSecret !== null && !isValidSessionSecret(sessionSecret)) {
        throw new Error("Session secret must be 128 lowercase hexadecimal characters");
    }
    if ((sessionId === null) !== (sessionSecret === null)) {
        throw new Error("Session link must include both an ID and secret");
    }
    if (sessionId && !workerBaseUrl) {
        throw new Error("Session mode requires a configured Worker URL");
    }

    return {
        sessionId,
        sessionSecret,
        workerBaseUrl,
    };
}

function containsSessionState(params) {
    return params.has("session") || params.has("secret") || params.has("worker");
}

/** Accept secure Worker URLs, with HTTP limited to local development. */
export function normalizeWorkerBaseUrl(value) {
    const url = new URL(value);
    const localHostnames = new Set(["localhost", "127.0.0.1", "[::1]"]);
    const isLocalHttp = url.protocol === "http:" && localHostnames.has(url.hostname);
    if (url.protocol !== "https:" && !isLocalHttp) {
        throw new Error("Worker URL must use HTTPS (or HTTP on localhost)");
    }
    if (url.username || url.password || url.search || url.hash) {
        throw new Error("Worker URL must not contain credentials, query, or fragment");
    }
    if (url.pathname !== "/") {
        throw new Error("Worker URL must be an origin without a path");
    }
    return url.origin;
}

/** Build the only endpoint used for a validated Session mutation. */
export function buildSessionEndpoint(workerBaseUrl, sessionId) {
    if (!isValidSessionId(sessionId)) {
        throw new Error("Session ID must be 64 lowercase hexadecimal characters");
    }
    const workerOrigin = normalizeWorkerBaseUrl(workerBaseUrl);
    return `${workerOrigin}/sessions/${sessionId}`;
}

function isValidSessionId(value) {
    return typeof value === "string" && /^[0-9a-f]{64}$/.test(value);
}

function isValidSessionSecret(value) {
    return typeof value === "string" && /^[0-9a-f]{128}$/.test(value);
}
