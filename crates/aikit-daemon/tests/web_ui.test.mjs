import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { setImmediate } from "node:timers/promises";
import { runInNewContext } from "node:vm";

const appSource = readFileSync(new URL("../assets/app.js", import.meta.url), "utf8");

class Element {
  children = [];
  attributes = {};
  style = {};

  set textContent(value) {
    this.text = value;
    this.children = [];
  }

  get textContent() { return this.text; }
  setAttribute(name, value) { this.attributes[name] = value; }
  appendChild(child) { this.children.push(child); }
  addEventListener() {}
  tagName = "div";
}

function response(body, status = 200) {
  const snapshot = structuredClone(body);
  return { status, ok: status === 200, json: async () => snapshot };
}

async function mountTargets(overrides = {}) {
  const config = {
    providers: [],
    active_selection: null,
    targets: [{ id: "claude", enabled: false }, { id: "codex", enabled: true }],
    claude_pin_models: false,
    context_1m: true,
    bypass_permissions: false,
    max_thinking_effort: false,
    ...overrides
  };
  const nodes = new Map();
  const requests = [];
  const document = {
    getElementById(id) {
      if (!nodes.has(id)) nodes.set(id, new Element());
      return nodes.get(id);
    },
    createElement(tag) {
      const element = new Element();
      element.tagName = tag;
      return element;
    }
  };

  runInNewContext(appSource, {
    document,
    localStorage: { getItem() { return "test-token"; } },
    setTimeout() {},
    fetch(path, options = {}) {
      if (path === "/api/ping") return Promise.resolve(response({}));
      if (path === "/api/health") return Promise.resolve(response({ version: "test", uptime_seconds: 0 }));
      if (path === "/api/config") return Promise.resolve(response(config));
      return new Promise((resolve) => {
        requests.push({ path, method: options.method, body: JSON.parse(options.body), resolve });
      });
    }
  });
  await setImmediate();

  return {
    config,
    requests,
    checkbox(label) {
      const row = nodes.get("targets-body").children.find((node) => node.children[1] && node.children[1].textContent === label);
      assert.ok(row, `Checkbox not found: ${label}`);
      return row.children[0];
    },
    hasGroupHeader(text) {
      return nodes.get("targets-body").children.some((node) => node.tagName === "h3" && node.textContent === text);
    }
  };
}

test("targets render an options group header after the target checkboxes", async () => {
  const page = await mountTargets();
  assert.ok(page.hasGroupHeader("选项"), "expected an 选项 group header in targets-body");
});

for (const [flag, label] of [
  ["claude_pin_models", "固定所有 Claude 模型"],
  ["context_1m", "1M 上下文"],
  ["bypass_permissions", "Bypass 权限（危险）"],
  ["max_thinking_effort", "最高思考强度"]
]) {
  for (const initial of [false, true]) {
    test(`${flag} submits the current checkbox value from initial ${initial}`, async () => {
      const page = await mountTargets({ [flag]: initial });
      assert.equal(page.checkbox(label).checked, initial);

      for (const checked of [!initial, initial, !initial, initial]) {
        const checkbox = page.checkbox(label);
        checkbox.checked = checked;
        checkbox.onchange();

        const request = page.requests.shift();
        assert.equal(request.path, "/api/targets");
        assert.equal(request.method, "PUT");
        assert.deepEqual(request.body, { [flag]: checked });
        assert.equal(page.checkbox(label).checked, checked, "optimistic checkbox state");

        page.config[flag] = checked;
        request.resolve(response(page.config));
        await setImmediate();
        assert.equal(page.checkbox(label).checked, checked, "saved checkbox state");
      }
    });
  }
}

test("a failed target update restores the persisted checkbox state", async () => {
  const page = await mountTargets();
  const label = "Bypass 权限（危险）";
  const checkbox = page.checkbox(label);
  checkbox.checked = true;
  checkbox.onchange();
  assert.equal(page.checkbox(label).checked, true);

  page.requests.shift().resolve(response({ error: "save failed" }, 500));
  await setImmediate();
  assert.equal(page.checkbox(label).checked, false);
});
