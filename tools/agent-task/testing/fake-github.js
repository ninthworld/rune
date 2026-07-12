/**
 * An in-memory GitHub, faked at the `fetch` boundary.
 *
 * The runner's whole safety story is about *which* mutations happen and in what order, so
 * the tests assert against real request traffic rather than against mocked methods: a fake
 * that only stubbed `GitHub.createBranch` would happily pass while the runner labelled an
 * issue it never claimed.
 */
export function fakeGitHub({ issues = {}, refs = { "heads/main": "base000" }, aheadBy = {} } = {}) {
  const state = {
    issues: structuredClone(issues),
    refs: { ...refs },
    aheadBy: { ...aheadBy },
    comments: [],
    calls: [],
  };

  // 204 must carry no body at all — `new Response("", {status: 204})` throws.
  const json = (status, body) =>
    new Response(body === undefined ? null : JSON.stringify(body), {
      status,
      headers: { "content-type": "application/json" },
    });

  state.fetch = async (url, init = {}) => {
    const method = init.method || "GET";
    const path = new URL(url).pathname.replace("/repos/ninthworld/rune", "");
    const body = init.body ? JSON.parse(init.body) : null;
    state.calls.push(`${method} ${path}`);

    let m;
    if ((m = /^\/issues\/(\d+)$/.exec(path)) && method === "GET") {
      const issue = state.issues[m[1]];
      return issue ? json(200, issue) : json(404, { message: "Not Found" });
    }
    if ((m = /^\/issues\/(\d+)\/labels$/.exec(path)) && method === "POST") {
      const issue = state.issues[m[1]];
      issue.labels = [...issue.labels, ...body.labels.map((name) => ({ name }))];
      return json(200, issue.labels);
    }
    if ((m = /^\/issues\/(\d+)\/labels\/(.+)$/.exec(path)) && method === "DELETE") {
      const issue = state.issues[m[1]];
      const label = decodeURIComponent(m[2]);
      if (!issue.labels.some((l) => l.name === label)) return json(404, { message: "Label does not exist" });
      issue.labels = issue.labels.filter((l) => l.name !== label);
      return json(200, issue.labels);
    }
    if ((m = /^\/issues\/(\d+)\/comments$/.exec(path)) && method === "POST") {
      state.comments.push({ issue: Number(m[1]), body: body.body });
      return json(201, { id: state.comments.length });
    }
    if ((m = /^\/git\/ref\/(.+)$/.exec(path)) && method === "GET") {
      const sha = state.refs[decodeURIComponent(m[1])];
      return sha ? json(200, { object: { sha } }) : json(404, { message: "Not Found" });
    }
    if (path === "/git/refs" && method === "POST") {
      const ref = body.ref.replace("refs/", "");
      if (state.refs[ref]) return json(422, { message: "Reference already exists" });
      state.refs[ref] = body.sha;
      return json(201, { ref: body.ref });
    }
    if ((m = /^\/git\/refs\/(.+)$/.exec(path)) && method === "DELETE") {
      const ref = decodeURIComponent(m[1]);
      if (!state.refs[ref]) return json(404, { message: "Not Found" });
      delete state.refs[ref];
      return json(204);
    }
    if ((m = /^\/compare\/main\.\.\.(.+)$/.exec(path)) && method === "GET") {
      return json(200, { ahead_by: state.aheadBy[decodeURIComponent(m[1])] || 0 });
    }
    return json(404, { message: `fake-github: unrouted ${method} ${path}` });
  };

  return state;
}

export function anIssue(overrides = {}) {
  return {
    number: 186,
    title: "tooling: implement provider-neutral issue runner",
    state: "open",
    body: "### Acceptance criteria\n\n- [ ] Implement the runner.\n",
    labels: [{ name: "agent-task" }, { name: "status:ready" }, { name: "area:ci" }],
    ...overrides,
  };
}
