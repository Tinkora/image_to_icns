/** Read session parameters and remove sensitive fragments from browser history. */
export function readSessionParams(location, history, configuredWorker) {
    const fragmentParams = new URLSearchParams(location.hash.slice(1));
    const queryParams = new URLSearchParams(location.search);
    const fragmentContainsSessionState = containsSessionState(fragmentParams);
    const queryContainsSessionState = containsSessionState(queryParams);
    const params = fragmentContainsSessionState ? fragmentParams : queryParams;
    const sessionId = params.get("session");
    const sessionSecret = params.get("secret");
    const worker = configuredWorker || params.get("worker");

    if (fragmentContainsSessionState || queryContainsSessionState) {
        queryParams.delete("session");
        queryParams.delete("secret");
        queryParams.delete("worker");
        const remainingQuery = queryParams.toString();
        const remainingFragment = fragmentContainsSessionState ? "" : location.hash;
        const cleanUrl = `${location.pathname}${remainingQuery ? `?${remainingQuery}` : ""}${remainingFragment}`;
        history.replaceState(history.state, "", cleanUrl);
    }

    return {
        sessionId,
        sessionSecret,
        workerBaseUrl: worker ? normalizeWorkerBaseUrl(worker) : null,
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
    return url.href.replace(/\/$/, "");
}
