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
