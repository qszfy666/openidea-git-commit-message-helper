//! Notification strings for the plugin. The IDE has no plugin locale
//! catalogue yet, so the plugin follows the IDE UI language from `host::locale`.

use crate::config::UiLanguage;

#[derive(Clone, Copy, Debug)]
pub struct Strings {
    pub no_repository: &'static str,
    pub no_changes: &'static str,
    pub no_draft: &'static str,
    pub not_configured: &'static str,
    pub collecting: &'static str,
    pub generating: &'static str,
    pub generating_now: &'static str,
    pub generated: &'static str,
    pub canceled: &'static str,
    pub stale: &'static str,
    pub bad_response: &'static str,
    pub error_prefix: &'static str,
    pub test_ok: &'static str,
    pub requirements_prompt: &'static str,
    pub pick_model: &'static str,
    pub model_hint: &'static str,
    pub model_copy_failed: &'static str,
    pub request_url: &'static str,
    pub key_slot: &'static str,
    pub template_fallback: &'static str,
    pub preview_fields_prompt: &'static str,
    pub preview_template_prompt: &'static str,
    pub preview_result: &'static str,
    pub preview_error: &'static str,
    pub template_help: &'static str,
    pub copy_ok: &'static str,
    pub copy_failed: &'static str,
    pub thinking_preset: &'static str,
    pub thinking_selected: &'static str,
    pub thinking_available: &'static str,
    pub thinking_unavailable: &'static str,
    pub mapping_error: &'static str,
    pub thinking_help: &'static str,
}

const ZH: Strings = Strings {
    no_repository: "当前窗口没有打开的 Git 仓库。",
    no_changes: "提交面板里没有勾选任何变更。",
    no_draft: "提交信息是空的，没有可格式化的内容。",
    not_configured: "还没有配置 LLM API Key。请在 Settings › Plugins 的「已安装」里选中本插件，在右侧「密钥」中填入 LLM API key 并保存。",
    collecting: "正在读取提交面板中的变更…",
    generating: "正在调用模型…",
    generating_now: "正在生成提交信息…",
    generated: "提交信息已生成，请检查后提交。",
    canceled: "已取消。",
    stale: "提交面板内容已变化，未覆盖你当前输入的提交信息。",
    bad_response: "模型返回的内容无法解析为提交信息 JSON，请重试或换一个模型。",
    error_prefix: "生成失败",
    test_ok: "LLM 连接正常。",
    requirements_prompt: "本次提交的补充要求（业务背景、Bug 说明、issue、Skip CI 等；留空取消）",
    pick_model: "选择模型",
    model_hint: "模型 ID 已复制到剪贴板，请粘贴到 LLM Settings 的 Model。",
    model_copy_failed: "未能写入剪贴板，请手动把这个模型 ID 填到 LLM Settings 的 Model。",
    request_url: "请求地址",
    key_slot: "密钥槽",
    template_fallback: "模板无效，已用内置 Default 渲染：",
    preview_fields_prompt: "选择哪些示例字段有值（未选中的字段为空，条件段不会输出）",
    preview_template_prompt: "编辑模板后确认即可预览，可反复修改；取消对话框结束预览",
    preview_result: "模板预览",
    preview_error: "模板错误",
    template_help: "输出模板变量：{{type}}、{{scope}}、{{subject}}、{{body}}、{{changes}}、{{closes}}、{{skipCi}}、{{header}}（即 type(scope): subject）。\n条件段：{{#body}}…{{/body}} 仅在字段非空时输出，以上任一变量都可作为段名。静态文本原样保留，因此最后一行可以是 footer。\n示例：{{header}}{{#body}}\n\n{{body}}{{/body}}{{#closes}}\n\nCloses {{closes}}{{/closes}}\n错误：未知变量、未闭合的 '{{'、段名不匹配、嵌套段或段未闭合。Preview 会显示错误；生成时会回退内置 Default 并在通知里说明。\n恢复：运行本页的 Restore Default 复制内置模板，或清空 Default template 选择。",
    copy_ok: "已复制到剪贴板，请粘贴到设置中：",
    copy_failed: "未能写入剪贴板，请手动复制下面的文本：",
    thinking_preset: "思考预设",
    thinking_selected: "当前等级",
    thinking_available: "可用等级",
    thinking_unavailable: "当前等级没有对应映射，请求将按 Model default 处理（不发送思考参数）。",
    mapping_error: "映射错误",
    thinking_help: "思考参数按模型档案配置。\n等级：DEFAULT（不发送）、DISABLED、ENABLED、LOW、MEDIUM、HIGH、MAX。预设：model-default、auto、openai-effort、nested-effort、thinking-object、enable-thinking、anthropic-budget、anthropic-adaptive、custom。\n映射：按等级名作键的 JSON 对象，每个值是非空的请求片段，例如 {\"HIGH\":{\"reasoning_effort\":\"high\"}}。$thinking_budget 只能出现在 thinking.budget_tokens，会按 Max response tokens 的一部分解析（不小于 1024 且小于上限）。保留字段（model、messages、system、temperature、max_tokens 等）、null 值、超过 12 层嵌套和超过 64 KiB 的映射都会被拒绝。\n恢复：清空 Thinking mapping 列即使用预设模板（下方已复制）；Thinking Levels 列出当前映射实际提供的等级。请求诊断会显示实际发送的内容。",
};

const EN: Strings = Strings {
    no_repository: "No Git repository is open in this window.",
    no_changes: "No changes are included in the commit panel.",
    no_draft: "The commit message is empty; there is nothing to format.",
    not_configured: "No LLM API key yet. In Settings › Plugins › Installed, select Git Commit Message Helper, fill in the LLM API key under Secrets, and save.",
    collecting: "Reading the changes in the commit panel…",
    generating: "Calling the model…",
    generating_now: "Generating commit message…",
    generated: "Commit message generated. Review it before committing.",
    canceled: "Cancelled.",
    stale: "The commit draft changed; your current message was not overwritten.",
    bad_response: "The model did not return valid commit JSON. Try again or use another model.",
    error_prefix: "Generation failed",
    test_ok: "LLM connection OK.",
    requirements_prompt: "Additional requirements for this commit (business background, bug report, issue, Skip CI; leave empty to cancel)",
    pick_model: "Pick a model",
    model_hint: "The model id is in the clipboard; paste it into LLM Settings › Model.",
    model_copy_failed: "The clipboard could not be written; type this model id into LLM Settings › Model.",
    request_url: "Request URL",
    key_slot: "Key slot",
    template_fallback: "Invalid template; rendered with the built-in Default instead:",
    preview_fields_prompt: "Choose which example fields have values (unselected fields stay empty, so their sections are skipped)",
    preview_template_prompt: "Edit the template and confirm to preview; edit again to iterate. Dismiss to stop.",
    preview_result: "Template preview",
    preview_error: "Template error",
    template_help: "Commit template variables: {{type}}, {{scope}}, {{subject}}, {{body}}, {{changes}}, {{closes}}, {{skipCi}}, {{header}} (= type(scope): subject).\nConditional sections: {{#body}}…{{/body}} are emitted only when that field is non-empty; any variable above can name a section. Plain text is kept, so the last line can be a footer.\nExample: {{header}}{{#body}}\n\n{{body}}{{/body}}{{#closes}}\n\nCloses {{closes}}{{/closes}}\nErrors: unknown variable, unclosed '{{', mismatched, nested or unclosed section. Preview shows the error; generation falls back to the built-in Default.\nRestore: run Restore Default on this page to copy the built-in template, or clear Default template.",
    copy_ok: "Copied to the clipboard; paste it into the setting:",
    copy_failed: "Could not write to the clipboard; copy the text below by hand:",
    thinking_preset: "Preset",
    thinking_selected: "Selected level",
    thinking_available: "Available levels",
    thinking_unavailable: "The selected level has no mapping; the request uses Model default and sends no thinking parameters.",
    mapping_error: "Mapping error",
    thinking_help: "Thinking parameters are per model profile.\nLevel: DEFAULT (send nothing), DISABLED, ENABLED, LOW, MEDIUM, HIGH, MAX. Preset: model-default, auto, openai-effort, nested-effort, thinking-object, enable-thinking, anthropic-budget, anthropic-adaptive, custom.\nMapping: a JSON object keyed by level; each value is a non-empty request fragment, e.g. {\"HIGH\":{\"reasoning_effort\":\"high\"}}. $thinking_budget is valid only at thinking.budget_tokens and resolves to a share of Max response tokens (>= 1024 and < the limit). Reserved fields (model, messages, system, temperature, max_tokens, …), null values, nesting deeper than 12 levels and mappings over 64 KiB are rejected.\nRestore: clear the Thinking mapping column to use the preset template (copied below), or run Thinking Levels to see which levels the mapping offers. Request diagnostics show what was actually sent.",
};

pub fn strings(language: UiLanguage) -> &'static Strings {
    if language.is_zh() { &ZH } else { &EN }
}

/// Clips text for a notification body.
pub fn clip(text: &str, max: usize) -> String {
    let mut chars = text.chars();
    let clipped: String = chars.by_ref().take(max).collect();
    if chars.next().is_some() {
        format!("{clipped}…")
    } else {
        clipped
    }
}
