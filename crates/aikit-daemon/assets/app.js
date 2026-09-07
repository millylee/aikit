(function () {
  "use strict";

  var TOKEN_KEY = "aikit_token";
  var state = { config: null, providerId: null, mode: "view" };
  var targetsRequestId = 0;

  function $(id) { return document.getElementById(id); }
  function el(tag, attrs, children) {
    var node = document.createElement(tag);
    for (var key in attrs || {}) {
      if (key === "text") node.textContent = attrs[key];
      else if (key === "onclick") node.onclick = attrs[key];
      else node.setAttribute(key, attrs[key]);
    }
    (children || []).forEach(function (child) { node.appendChild(child); });
    return node;
  }

  function setStatus(message, kind) {
    var container = $("toasts");
    if (!container) return;
    var toast = el("div", { "class": "toast " + (kind || "") }, [el("span", { text: message })]);
    container.appendChild(toast);
    setTimeout(function () { toast.classList.add("leaving"); }, 2600);
    setTimeout(function () { if (toast.parentNode) toast.parentNode.removeChild(toast); }, 3000);
  }

  function api(method, path, body) {
    return fetch(path, {
      method: method,
      headers: {
        "Authorization": "Bearer " + localStorage.getItem(TOKEN_KEY),
        "Content-Type": "application/json"
      },
      body: body ? JSON.stringify(body) : undefined
    }).then(function (response) {
      if (response.status === 401) { showLogin(); throw new Error("未授权"); }
      return response.json().then(function (data) {
        if (!response.ok) throw new Error(data.error || ("HTTP " + response.status));
        return data;
      });
    });
  }

  function showLogin() {
    $("app").style.display = "none";
    $("login").style.display = "block";
    $("token-input").focus();
  }

  function showApp() {
    $("login").style.display = "none";
    $("app").style.display = "flex";
  }

  function login() {
    var token = $("token-input").value.trim();
    if (!token) return;
    localStorage.setItem(TOKEN_KEY, token);
    api("GET", "/api/ping").then(function () {
      showApp();
      refreshAll();
    }).catch(function () {
      localStorage.removeItem(TOKEN_KEY);
      setStatus("令牌无效，请重试", "error");
    });
  }

  function refreshAll() {
    fetch("/api/health").then(function (r) { return r.json(); }).then(function (health) {
      $("health-meta").textContent = "v" + health.version + " · 已运行 " + health.uptime_seconds + " 秒";
    });
    var requestIdAtStart = targetsRequestId;
    api("GET", "/api/config").then(function (config) {
      if (targetsRequestId !== requestIdAtStart) {
        // A target toggle raced this refresh; keep the toggled fields.
        config.targets = state.config.targets;
        config.claude_pin_models = state.config.claude_pin_models;
        config.context_1m = state.config.context_1m;
        config.bypass_permissions = state.config.bypass_permissions;
      }
      state.config = config;
      if (!state.providerId && config.providers.length) {
        state.providerId = config.active_selection
          ? config.active_selection.provider_id
          : config.providers[0].id;
      }
      renderProviders();
      renderDetail();
      renderTargets();
    }).catch(function (err) { setStatus("加载配置失败：" + err.message, "error"); });
  }

  function isActive(providerId, keyId, model) {
    var active = state.config.active_selection;
    return active && active.provider_id === providerId &&
      (!keyId || active.api_key_id === keyId) &&
      (!model || active.model_id === model);
  }

  function renderProviders() {
    var list = $("provider-list");
    list.textContent = "";
    state.config.providers.forEach(function (provider) {
      var badge = isActive(provider.id) ? el("span", { "class": "badge", text: "●" }) : el("span", { text: " " });
      var item = el("li", {
        "class": provider.id === state.providerId ? "active" : "",
        onclick: function () { state.providerId = provider.id; state.mode = "view"; renderProviders(); renderDetail(); }
      }, [badge, el("span", { text: provider.name + (provider.enabled ? "" : "（已停用）") })]);
      list.appendChild(item);
    });
  }

  function selectedProvider() {
    if (!state.config) return null;
    return state.config.providers.find(function (p) { return p.id === state.providerId; }) || null;
  }

  function renderDetail() {
    var body = $("detail-body");
    var title = $("detail-title");
    body.textContent = "";
    var provider = selectedProvider();
    if (!provider) { title.textContent = "详情"; body.appendChild(el("p", { "class": "muted", text: "请选择供应商" })); return; }
    title.textContent = "供应商：" + provider.name;

    if (state.mode === "edit") { renderProviderForm(body, provider); return; }

    var info = el("div", {}, [
      el("div", { "class": "row" }, [el("label", { text: "ID" }), el("code", { text: provider.id })]),
      el("div", { "class": "row" }, [el("label", { text: "Base URL" }), el("code", { text: provider.base_url })]),
      el("div", { "class": "row" }, [
        el("label", { text: "状态" }),
        el("span", { text: provider.enabled ? "已启用" : "已停用" }),
        el("button", { "class": "small", text: "编辑", onclick: function () { state.mode = "edit"; renderDetail(); } })
      ])
    ]);
    body.appendChild(info);

    body.appendChild(el("h2", { text: "API 密钥" }));
    var keys = el("ul", { "class": "plain keys-list" });
    provider.api_keys.forEach(function (key) {
      keys.appendChild(el("li", { "class": isActive(provider.id, key.id) ? "active" : "" }, [
        el("span", { text: key.name + " " }),
        el("span", { "class": "masked", text: key.value_masked }),
        el("button", { "class": "small", text: "选用", onclick: function () { selectKey(provider, key); } }),
        el("button", { "class": "small danger", text: "替换", onclick: function () { replaceKey(provider, key); } }),
        el("button", { "class": "small danger", text: "删除", onclick: function () {
          if (!confirm("删除密钥 " + key.name + "？")) return;
          api("DELETE", "/api/providers/" + provider.id + "/keys/" + key.id)
            .then(function () { setStatus("密钥已删除", "ok"); refreshAll(); })
            .catch(function (err) { setStatus("删除失败：" + err.message, "error"); });
        } })
      ]));
    });
    body.appendChild(keys);
    body.appendChild(el("div", { "class": "row" }, [
      el("button", { "class": "small", text: "+ 新增密钥", onclick: function () { addKey(provider); } })
    ]));

    body.appendChild(el("h2", { text: "模型" }));
    var models = provider.models_cache ? provider.models_cache.models : [];
    var manual = provider.manual_models || [];
    var all = models.concat(manual.filter(function (m) { return models.indexOf(m) < 0; }));
    var modelList = el("ul", { "class": "plain model-list" });
    if (!all.length) modelList.appendChild(el("li", {}, [el("span", { "class": "muted", text: "暂无模型，请先刷新或手动添加" })]));
    all.forEach(function (model) {
      modelList.appendChild(el("li", { "class": isActive(provider.id, null, model) ? "active" : "" }, [
        el("span", { text: model, onclick: function () { selectModel(provider, model); }, style: "flex:1" }),
        el("span", { "class": "badge", text: isActive(provider.id, null, model) ? "当前" : "" })
      ]));
    });
    body.appendChild(modelList);
    body.appendChild(el("div", { "class": "row" }, [
      el("button", { "class": "small", text: "刷新模型列表", onclick: function () { refreshModels(provider); } })
    ]));
    if (provider.models_cache && provider.models_cache.last_error) {
      body.appendChild(el("p", { "class": "inline-error", text: "上次刷新失败：" + provider.models_cache.last_error }));
    }
  }

  function renderProviderForm(body, provider) {
    function field(label, id, value, type) {
      var input = el("input", { type: type || "text" });
      input.value = value;
      input.id = id;
      return el("div", { "class": "row" }, [el("label", { text: label }), input]);
    }
    body.appendChild(field("名称", "f-name", provider.name));
    body.appendChild(field("Base URL", "f-url", provider.base_url, "url"));
    var enabled = el("input", { type: "checkbox" });
    enabled.checked = provider.enabled;
    body.appendChild(el("div", { "class": "check" }, [enabled, el("span", { text: "启用" })]));
    body.appendChild(el("div", { "class": "row" }, [
      el("button", { "class": "primary", text: "保存", onclick: function () {
        api("PUT", "/api/providers/" + provider.id, {
          name: $("f-name").value, base_url: $("f-url").value, enabled: enabled.checked
        }).then(function () { state.mode = "view"; setStatus("供应商已保存", "ok"); refreshAll(); })
          .catch(function (err) { setStatus("保存失败：" + err.message, "error"); });
      } }),
      el("button", { text: "取消", onclick: function () { state.mode = "view"; renderDetail(); } }),
      el("button", { "class": "danger", text: "删除供应商", onclick: function () {
        if (!confirm("删除供应商 " + provider.name + "？")) return;
        api("DELETE", "/api/providers/" + provider.id)
          .then(function () { state.providerId = null; state.mode = "view"; setStatus("供应商已删除", "ok"); refreshAll(); })
          .catch(function (err) { setStatus("删除失败：" + err.message, "error"); });
      } })
    ]));
  }

  function promptKey(title, callback) {
    var name = prompt(title + "：密钥名称");
    if (name === null) return;
    var value = prompt(title + "：密钥值（明文，仅存储到本地配置）");
    if (value === null) return;
    callback(name.trim(), value.trim());
  }

  function addKey(provider) {
    promptKey("新增密钥", function (name, value) {
      api("POST", "/api/providers/" + provider.id + "/keys", { name: name, value: value })
        .then(function () { setStatus("密钥已新增", "ok"); refreshAll(); })
        .catch(function (err) { setStatus("新增失败：" + err.message, "error"); });
    });
  }

  function replaceKey(provider, key) {
    promptKey("替换密钥 " + key.name, function (name, value) {
      api("PUT", "/api/providers/" + provider.id + "/keys/" + key.id, { name: name, value: value })
        .then(function () { setStatus("密钥已替换", "ok"); refreshAll(); })
        .catch(function (err) { setStatus("替换失败：" + err.message, "error"); });
    });
  }

  function selectKey(provider, key) {
    var active = state.config.active_selection || { provider_id: provider.id, api_key_id: key.id, model_id: "" };
    api("PUT", "/api/selection", {
      provider_id: provider.id, api_key_id: key.id, model_id: active.model_id
    }).then(function () { setStatus("已选用密钥 " + key.name, "ok"); refreshAll(); })
      .catch(function (err) { setStatus("选用失败：" + err.message, "error"); });
  }

  function selectModel(provider, model) {
    var keys = provider.api_keys;
    if (!keys.length) { setStatus("请先为该供应商添加密钥", "error"); return; }
    var active = state.config.active_selection;
    var keyId = active && active.provider_id === provider.id ? active.api_key_id : keys[0].id;
    api("PUT", "/api/selection", {
      provider_id: provider.id, api_key_id: keyId, model_id: model
    }).then(function () { setStatus("已选用模型 " + model, "ok"); refreshAll(); })
      .catch(function (err) { setStatus("选用失败：" + err.message, "error"); });
  }

  function refreshModels(provider) {
    setStatus("正在刷新模型列表…");
    var active = state.config.active_selection;
    var keyId = active && active.provider_id === provider.id ? active.api_key_id : (provider.api_keys[0] || {}).id;
    api("POST", "/api/models/refresh", { provider_id: provider.id, api_key_id: keyId })
      .then(function (result) { setStatus("已刷新 " + result.refreshed + " 个模型", "ok"); refreshAll(); })
      .catch(function (err) { setStatus("刷新失败：" + err.message, "error"); });
  }

  function renderTargets() {
    var body = $("targets-body");
    body.textContent = "";
    var config = state.config;
    config.targets.forEach(function (target) {
      var check = el("input", { type: "checkbox" });
      check.checked = target.enabled;
      check.onchange = function () { toggleTargets({ targets: [{ id: target.id, enabled: check.checked }] }); };
      body.appendChild(el("label", { "class": "check" }, [check, el("span", { text: targetDisplayName(target.id) })]));
    });
    body.appendChild(el("h3", { text: "选项" }));
    [["claude_pin_models", "固定所有 Claude 模型", config.claude_pin_models],
     ["context_1m", "1M 上下文", config.context_1m],
     ["bypass_permissions", "Bypass 权限（危险）", config.bypass_permissions]].forEach(function (item) {
      var check = el("input", { type: "checkbox" });
      check.checked = item[2];
      check.onchange = function () {
        var payload = {};
        payload[item[0]] = check.checked;
        toggleTargets(payload);
      };
      body.appendChild(el("label", { "class": "check" }, [check, el("span", { text: item[1] })]));
    });
  }

  function targetDisplayName(id) {
    return { claude: "Claude Code", codex: "Codex CLI" }[id] || id;
  }

  // Optimistically apply the payload to local state so the checkbox reflects
  // the click immediately, then send it. Only the newest request may write
  // its response back, so a slow earlier response cannot revert later toggles.
  function toggleTargets(payload) {
    applyTargetsPayloadLocally(payload);
    renderTargets();
    var id = ++targetsRequestId;
    api("PUT", "/api/targets", payload)
      .then(function (config) {
        if (id !== targetsRequestId) return;
        state.config.targets = config.targets;
        state.config.claude_pin_models = config.claude_pin_models;
        state.config.context_1m = config.context_1m;
        state.config.bypass_permissions = config.bypass_permissions;
        renderTargets();
        setStatus("已更新", "ok");
      })
      .catch(function (err) {
        if (id !== targetsRequestId) return;
        refreshTargetsOnly();
        setStatus("更新失败：" + err.message, "error");
      });
  }

  function applyTargetsPayloadLocally(payload) {
    (payload.targets || []).forEach(function (update) {
      var target = state.config.targets.find(function (t) { return t.id === update.id; });
      if (target) target.enabled = update.enabled;
    });
    ["claude_pin_models", "context_1m", "bypass_permissions"].forEach(function (key) {
      if (key in payload) state.config[key] = payload[key];
    });
  }

  function refreshTargetsOnly() {
    api("GET", "/api/config").then(function (config) {
      state.config.targets = config.targets;
      state.config.claude_pin_models = config.claude_pin_models;
      state.config.context_1m = config.context_1m;
      state.config.bypass_permissions = config.bypass_permissions;
      renderTargets();
    }).catch(function () {});
  }

  function applySelection() {
    setStatus("正在应用…");
    api("POST", "/api/apply")
      .then(function (report) {
        if (report.target_results && report.target_results.length) {
          report.target_results.forEach(function (result) {
            var ok = result.status === "applied";
            setStatus(targetDisplayName(result.target_id) + "：" + result.status, ok ? "ok" : "error");
          });
        }
        if (report.succeeded === 0 && report.failed === 0) {
          setStatus("未应用任何目标：请先勾选【应用目标】中的项", "error");
        } else {
          setStatus(report.message, report.failed > 0 ? "error" : "ok");
        }
      })
      .catch(function (err) { setStatus("应用失败：" + err.message, "error"); });
  }

  function importScan() {
    setStatus("正在扫描…");
    api("POST", "/api/import/scan").then(function (plan) {
      var body = $("import-body");
      body.textContent = "";
      if (!plan.candidates.length) { setStatus("未发现可导入的配置", "ok"); return; }
      setStatus("发现 " + plan.candidates.length + " 个可导入项", "ok");
      var list = el("ul", { "class": "plain candidates" });
      var checks = [];
      plan.candidates.forEach(function (candidate, index) {
        var check = el("input", { type: "checkbox" });
        check.checked = true;
        checks.push({ check: check, candidate: candidate });
        list.appendChild(el("li", {}, [
          el("label", { "class": "check" }, [
            check,
            el("span", { text: candidate.provider_name + "（" + candidate.source + "）" })
          ])
        ]));
      });
      body.appendChild(list);
      body.appendChild(el("div", { "class": "row" }, [
        el("button", { "class": "small primary", text: "导入选中项", onclick: function () {
          var selected = checks.filter(function (item) { return item.check.checked; })
            .map(function (item) { return item.candidate; });
          if (!selected.length) return;
          api("POST", "/api/import/apply", { candidates: selected })
            .then(function (result) {
              setStatus("导入完成：新增 " + result.added_providers + "，更新 " + result.updated_providers, "ok");
              $("import-body").textContent = "";
              refreshAll();
            })
            .catch(function (err) { setStatus("导入失败：" + err.message, "error"); });
        } })
      ]));
      plan.warnings.forEach(function (warning) { setStatus(warning, "error"); });
    }).catch(function (err) { setStatus("扫描失败：" + err.message, "error"); });
  }

  function checkUpdates() {
    setStatus("正在检查更新…");
    api("POST", "/api/updates/check")
      .then(function (outcome) { setStatus(outcome.message, "ok"); })
      .catch(function (err) { setStatus("检查失败：" + err.message, "error"); });
  }

  $("login-btn").onclick = login;
  $("token-input").addEventListener("keydown", function (event) { if (event.key === "Enter") login(); });
  $("logout-btn").onclick = function () { localStorage.removeItem(TOKEN_KEY); showLogin(); };
  $("apply-btn").onclick = applySelection;
  $("import-btn").onclick = importScan;
  $("updates-btn").onclick = checkUpdates;
  $("add-provider-btn").onclick = function () {
    var id = prompt("新增供应商：ID（英文标识）");
    if (!id) return;
    var name = prompt("新增供应商：名称");
    if (name === null) return;
    var baseUrl = prompt("新增供应商：Base URL");
    if (baseUrl === null) return;
    api("POST", "/api/providers", { id: id.trim(), name: name || id, base_url: baseUrl.trim(), enabled: true })
      .then(function (provider) { state.providerId = provider.id; setStatus("供应商已新增，请添加密钥", "ok"); refreshAll(); })
      .catch(function (err) { setStatus("新增失败：" + err.message, "error"); });
  };

  if (localStorage.getItem(TOKEN_KEY)) {
    api("GET", "/api/ping").then(showApp).then(refreshAll).catch(function () {});
  } else {
    showLogin();
  }
})();
