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
    claude_disable_betas: false,
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
        requests.push({ path, method: options.method, body: options.body ? JSON.parse(options.body) : null, resolve });
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
  ["max_thinking_effort", "最高思考强度"],
  ["claude_disable_betas", "禁用实验性 Beta（CC）"]
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

test("app.js avoids native browser dialogs", () => {
  assert.ok(!/\balert\(/.test(appSource), "alert() must not be used");
  assert.ok(!/\bconfirm\(/.test(appSource), "confirm() must not be used");
  assert.ok(!/(?<!\.)\bprompt\(/.test(appSource), "prompt() must not be used");
});

async function mountApp(overrides = {}) {
  const provider = {
    id: "p1",
    name: "Provider One",
    base_url: "https://example.com/v1",
    enabled: true,
    api_keys: [{ id: "k1", name: "Key", value_masked: "sk-1...xxx" }],
    manual_models: ["manual-model"],
    models_cache: { models: ["cached-model"], last_error: null },
    ...overrides.provider || {}
  };
  const config = {
    providers: [provider],
    active_selection: null,
    targets: [{ id: "claude", enabled: false }, { id: "codex", enabled: true }],
    claude_pin_models: false,
    context_1m: true,
    bypass_permissions: false,
    max_thinking_effort: false,
    claude_disable_betas: false,
    ...overrides.config || {}
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
        requests.push({ path, method: options.method, body: options.body ? JSON.parse(options.body) : null, resolve });
      });
    }
  });
  await setImmediate();
  return {
    config, requests, nodes,
    modalButtons() {
      const root = nodes.get("modal-root");
      const buttons = [];
      (function walk(node) {
        (node.children || []).forEach(function (child) {
          if (child.tagName === "button" && child.onclick) buttons.push(child);
          walk(child);
        });
      })(root);
      return buttons;
    },
    modalInputs() {
      const root = nodes.get("modal-root");
      const inputs = [];
      (function walk(node) {
        (node.children || []).forEach(function (child) {
          if (child.tagName === "input") inputs.push(child);
          walk(child);
        });
      })(root);
      return inputs;
    }
  };
}

test("add provider opens a form modal and submits one POST", async () => {
  const page = await mountApp();
  page.nodes.get("add-provider-btn").onclick();

  const inputs = page.modalInputs();
  assert.equal(inputs.length, 3, "ID / name / base URL fields in one modal");
  inputs[0].value = "newp";
  inputs[1].value = "New Provider";
  inputs[2].value = "https://new.example/v1";

  const confirm = page.modalButtons().find((button) => button.text === "确定");
  assert.ok(confirm, "confirm button present");
  confirm.onclick();

  const request = page.requests.shift();
  assert.equal(request.method, "POST");
  assert.equal(request.path, "/api/providers");
  assert.deepEqual(request.body, {
    id: "newp", name: "New Provider", base_url: "https://new.example/v1", enabled: true
  });
});

test("manual model add button opens modal and posts to the models endpoint", async () => {
  const page = await mountApp();
  const detail = page.nodes.get("detail-body");
  const addModel = findButtonByText(detail, "+ 手动添加模型");
  assert.ok(addModel, "manual add-model button rendered");
  addModel.onclick();

  const inputs = page.modalInputs();
  assert.equal(inputs.length, 1);
  inputs[0].value = "glm-5.4";
  page.modalButtons().find((button) => button.text === "确定").onclick();

  const request = page.requests.shift();
  assert.equal(request.method, "POST");
  assert.equal(request.path, "/api/providers/p1/models");
  assert.deepEqual(request.body, { model: "glm-5.4" });
});

function modelListRows(page) {
  const detail = page.nodes.get("detail-body");
  let list = null;
  (function walk(node) {
    (node.children || []).forEach(function (child) {
      if (child.tagName === "ul" && (child.attributes["class"] || "").indexOf("model-list") >= 0) list = child;
      walk(child);
    });
  })(detail);
  assert.ok(list, "model list rendered");
  return list.children;
}

test("delete button renders only for manual models", async () => {
  const page = await mountApp();
  const rows = modelListRows(page);
  assert.equal(rows.length, 2, "cached + manual model rows");
  const deletable = rows.filter((row) => row.children.some((child) => child.tagName === "button" && child.text === "删除"));
  assert.equal(deletable.length, 1, "only one row has a delete button");
  const rowText = deletable[0].children.map((child) => child.text).join("");
  assert.ok(rowText.indexOf("manual-model") >= 0, "the manual model row is deletable");
});

test("deleting a manual model asks for confirmation in a modal", async () => {
  const page = await mountApp();
  const rows = modelListRows(page);
  const deleteButton = rows
    .flatMap((row) => row.children)
    .find((child) => child.tagName === "button" && child.text === "删除");
  assert.ok(deleteButton, "manual model delete button present");
  deleteButton.onclick();

  const confirm = page.modalButtons().find((button) => button.text === "删除");
  assert.ok(confirm, "modal confirm button present");
  confirm.onclick();

  const request = page.requests.shift();
  assert.equal(request.method, "DELETE");
  assert.equal(request.path, "/api/providers/p1/models/manual-model");
});

function findButtonByText(container, text) {
  let found = null;
  (function walk(node) {
    (node.children || []).forEach(function (child) {
      if (child.tagName === "button" && child.text === text && child.onclick) found = child;
      walk(child);
    });
  })(container);
  return found;
}
