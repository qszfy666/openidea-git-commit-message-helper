//! Notification strings for the plugin. The IDE has no plugin locale
//! catalogue yet, so the plugin follows the IDE UI language from `host::locale`.

use crate::config::UiLanguage;

#[derive(Clone, Copy, Debug)]
pub struct Strings {
    pub no_repository: &'static str,
    pub no_changes: &'static str,
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
    pub key_slot: &'static str,
    pub template_fallback: &'static str,
    pub template_help: &'static str,
    pub copy_ok: &'static str,
    pub copy_failed: &'static str,
    pub quality_prefix: &'static str,
    pub quality_vague: &'static str,
    pub quality_filename: &'static str,
    pub quality_short: &'static str,
    pub list_separator: &'static str,
    // LLM errors; `llm` fills in the `{name}` placeholders.
    pub llm_no_key: &'static str,
    pub llm_no_model: &'static str,
    pub llm_no_base_url: &'static str,
    pub llm_budget_spent: &'static str,
    pub llm_request_failed: &'static str,
    pub llm_test_not_json: &'static str,
    pub llm_not_json: &'static str,
    pub llm_no_text: &'static str,
}

const ZH: Strings = Strings {
    no_repository: "当前窗口没有打开的 Git 仓库。",
    no_changes: "提交面板里没有勾选任何变更。",
    not_configured: "还没有配置 LLM API 密钥。请在 设置 › 插件 › 已安装 里选中本插件，在右侧「密钥」中填入「LLM API 密钥」并保存。",
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
    key_slot: "密钥槽",
    template_fallback: "模板无效，已用内置 Default 渲染：",
    template_help: "输出模板变量：{{type}}、{{scope}}、{{subject}}、{{body}}、{{changes}}、{{closes}}、{{skipCi}}、{{header}}（即 type(scope): subject）。\n条件段：{{#body}}…{{/body}} 仅在字段非空时输出，以上任一变量都可作为段名。静态文本原样保留，因此最后一行可以是 footer。\n示例：{{header}}{{#body}}\n\n{{body}}{{/body}}{{#closes}}\n\nCloses {{closes}}{{/closes}}\n错误：未知变量、未闭合的 '{{'、段名不匹配、嵌套段或段未闭合。生成时会回退内置 Default 并在通知里说明。\n恢复：运行本页的「恢复默认」复制内置模板，或清空「默认模板」选择。",
    copy_ok: "已复制到剪贴板，请粘贴到设置中：",
    copy_failed: "未能写入剪贴板，请手动复制下面的文本：",
    quality_prefix: "质量提示",
    quality_vague: "标题只重复了动作（如「修改代码」），没有说出具体改了什么",
    quality_filename: "标题只是改动文件名",
    quality_short: "标题过短（不足 12 个字符宽）",
    list_separator: "；",
    llm_no_key: "密钥槽 '{slot}' 中没有 API 密钥。请在 设置 › 插件 › 已安装 里选中 Git Commit Message Helper，在「密钥」中填入对应的「LLM API 密钥」。",
    llm_no_model: "「模型」为空。请选择服务商预设，或在 设置 › 插件 › Git Commit Message Helper 中填写「模型」。",
    llm_no_base_url: "「基础 URL」为空。请选择服务商预设，或在 设置 › 插件 › Git Commit Message Helper 中填写「基础 URL」。",
    llm_budget_spent: "{failure}（已尝试 {attempts} 次，任务时间预算已用完，不再重试）",
    llm_request_failed: "LLM 请求失败（HTTP {status}）：{detail}",
    llm_test_not_json: "LLM 测试返回的不是 JSON：{error}：{body}",
    llm_not_json: "LLM 返回的不是 JSON：{error}：{body}",
    llm_no_text: "LLM 响应中没有任何文本：{body}",
};

const EN: Strings = Strings {
    no_repository: "No Git repository is open in this window.",
    no_changes: "No changes are included in the commit panel.",
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
    key_slot: "Key slot",
    template_fallback: "Invalid template; rendered with the built-in Default instead:",
    template_help: "Commit template variables: {{type}}, {{scope}}, {{subject}}, {{body}}, {{changes}}, {{closes}}, {{skipCi}}, {{header}} (= type(scope): subject).\nConditional sections: {{#body}}…{{/body}} are emitted only when that field is non-empty; any variable above can name a section. Plain text is kept, so the last line can be a footer.\nExample: {{header}}{{#body}}\n\n{{body}}{{/body}}{{#closes}}\n\nCloses {{closes}}{{/closes}}\nErrors: unknown variable, unclosed '{{', mismatched, nested or unclosed section. Generation falls back to the built-in Default and the notification says why.\nRestore: run Restore Default on this page to copy the built-in template, or clear Default template.",
    copy_ok: "Copied to the clipboard; paste it into the setting:",
    copy_failed: "Could not write to the clipboard; copy the text below by hand:",
    quality_prefix: "Quality check",
    quality_vague: "the subject only restates the act (\"modify code\"), not what changed",
    quality_filename: "the subject is only a changed file name",
    quality_short: "the subject is shorter than 12 columns",
    list_separator: "; ",
    llm_no_key: "No API key in slot '{slot}'. In Settings › Plugins › Installed, select Git Commit Message Helper and fill in the matching LLM API key under Secrets.",
    llm_no_model: "Model is empty. Pick a provider preset or fill in Model in Settings › Plugins › Git Commit Message Helper.",
    llm_no_base_url: "Base URL is empty. Pick a provider preset or fill in Base URL in Settings › Plugins › Git Commit Message Helper.",
    llm_budget_spent: "{failure} (stopped after {attempts} attempts: the task time budget is spent)",
    llm_request_failed: "LLM request failed (HTTP {status}): {detail}",
    llm_test_not_json: "LLM test returned non-JSON response: {error}: {body}",
    llm_not_json: "LLM returned non-JSON response: {error}: {body}",
    llm_no_text: "LLM response did not contain any text: {body}",
};

const JA: Strings = Strings {
    no_repository: "このウィンドウに Git リポジトリが開かれていません。",
    no_changes: "コミット パネルに変更が含まれていません。",
    not_configured: "LLM API キーが未設定です。設定 › プラグイン › インストール済み で Git Commit Message Helper を選び、シークレットに LLM API キーを入力して保存してください。",
    collecting: "コミット パネルの変更を読み取っています…",
    generating: "モデルを呼び出しています…",
    generating_now: "コミットメッセージを生成しています…",
    generated: "コミットメッセージを生成しました。コミット前に確認してください。",
    canceled: "中止しました。",
    stale: "コミット ドラフトが変わりました。入力中のコミットメッセージは上書きしていません。",
    bad_response: "モデルの応答をコミット情報の JSON として解釈できませんでした。再試行するか別のモデルを使ってください。",
    error_prefix: "生成に失敗しました",
    test_ok: "LLM 接続は正常です。",
    requirements_prompt: "このコミットの追加要件（背景、不具合の内容、issue、Skip CI など。空のまま中止）",
    key_slot: "キー スロット",
    template_fallback: "テンプレートが不正なため、組み込みの既定テンプレートで描画しました:",
    template_help: "コミットテンプレートの変数: {{type}}、{{scope}}、{{subject}}、{{body}}、{{changes}}、{{closes}}、{{skipCi}}、{{header}}（= type(scope): subject）。\n条件セクション: {{#body}}…{{/body}} はその項目が空でないときだけ出力されます。上記のどの変数もセクション名に使えます。変数の外のテキストはそのまま残るため、最終行をフッターにできます。\n例: {{header}}{{#body}}\n\n{{body}}{{/body}}{{#closes}}\n\nCloses {{closes}}{{/closes}}\nエラー: 不明な変数、閉じていない '{{'、対応しないセクション名、入れ子のセクション、閉じられていないセクション。生成時は組み込みの既定テンプレートに戻して通知で理由を示します。\n復元: このページの「既定に戻す」で組み込みテンプレートをコピーするか、「既定のテンプレート」を空にしてください。",
    copy_ok: "クリップボードにコピーしました。設定に貼り付けてください:",
    copy_failed: "クリップボードに書き込めませんでした。下のテキストを手動でコピーしてください:",
    quality_prefix: "品質チェック",
    quality_vague: "タイトルが動作の言い換え（「コードを修正」など）だけで、何を変えたかを示していません",
    quality_filename: "タイトルが変更したファイル名だけになっています",
    quality_short: "タイトルが短すぎます（12 桁未満）",
    list_separator: "、",
    llm_no_key: "キー スロット '{slot}' に API キーがありません。設定 › プラグイン › インストール済み で Git Commit Message Helper を選び、シークレットに対応する LLM API キーを入力してください。",
    llm_no_model: "「モデル」が空です。プロバイダーのプリセットを選ぶか、設定 › プラグイン › Git Commit Message Helper で「モデル」を入力してください。",
    llm_no_base_url: "「ベース URL」が空です。プロバイダーのプリセットを選ぶか、設定 › プラグイン › Git Commit Message Helper で「ベース URL」を入力してください。",
    llm_budget_spent: "{failure}（{attempts} 回試行した後、タスクの時間予算を使い切ったため中止しました）",
    llm_request_failed: "LLM リクエストに失敗しました（HTTP {status}）: {detail}",
    llm_test_not_json: "LLM テストの応答が JSON ではありません: {error}: {body}",
    llm_not_json: "LLM の応答が JSON ではありません: {error}: {body}",
    llm_no_text: "LLM の応答にテキストが含まれていません: {body}",
};

const KO: Strings = Strings {
    no_repository: "이 창에 Git 저장소가 열려 있지 않습니다.",
    no_changes: "커밋 패널에 포함된 변경이 없습니다.",
    not_configured: "LLM API 키가 없습니다. 설정 › 플러그인 › 설치됨에서 Git Commit Message Helper를 선택하고 시크릿에 LLM API 키를 입력한 뒤 저장하세요.",
    collecting: "커밋 패널의 변경을 읽는 중…",
    generating: "모델을 호출하는 중…",
    generating_now: "커밋 메시지를 생성하는 중…",
    generated: "커밋 메시지를 생성했습니다. 커밋하기 전에 확인하세요.",
    canceled: "취소했습니다.",
    stale: "커밋 초안이 바뀌었습니다. 입력 중인 커밋 메시지는 덮어쓰지 않았습니다.",
    bad_response: "모델 응답을 커밋 정보 JSON으로 해석하지 못했습니다. 다시 시도하거나 다른 모델을 사용하세요.",
    error_prefix: "생성 실패",
    test_ok: "LLM 연결이 정상입니다.",
    requirements_prompt: "이 커밋의 추가 요구 사항(배경, 버그 설명, issue, Skip CI 등. 비워 두면 취소)",
    key_slot: "키 슬롯",
    template_fallback: "템플릿이 올바르지 않아 내장 기본 템플릿으로 렌더링했습니다:",
    template_help: "커밋 템플릿 변수: {{type}}, {{scope}}, {{subject}}, {{body}}, {{changes}}, {{closes}}, {{skipCi}}, {{header}}(= type(scope): subject).\n조건 섹션: {{#body}}…{{/body}}는 해당 항목이 비어 있지 않을 때만 출력됩니다. 위의 어떤 변수든 섹션 이름으로 쓸 수 있습니다. 변수 밖의 텍스트는 그대로 남으므로 마지막 줄을 footer로 쓸 수 있습니다.\n예: {{header}}{{#body}}\n\n{{body}}{{/body}}{{#closes}}\n\nCloses {{closes}}{{/closes}}\n오류: 알 수 없는 변수, 닫히지 않은 '{{', 짝이 맞지 않는 섹션 이름, 중첩 섹션, 닫히지 않은 섹션. 생성 시에는 내장 기본 템플릿으로 돌아가 알림에 이유를 적습니다.\n복원: 이 페이지의 '기본값 복원'으로 내장 템플릿을 복사하거나 '기본 템플릿'을 비우세요.",
    copy_ok: "클립보드에 복사했습니다. 설정에 붙여 넣으세요:",
    copy_failed: "클립보드에 쓰지 못했습니다. 아래 텍스트를 직접 복사하세요:",
    quality_prefix: "품질 점검",
    quality_vague: "제목이 동작을 다시 말할 뿐(\"코드 수정\" 등) 무엇이 바뀌었는지 말하지 않습니다",
    quality_filename: "제목이 변경된 파일 이름뿐입니다",
    quality_short: "제목이 너무 짧습니다(12칸 미만)",
    list_separator: ", ",
    llm_no_key: "키 슬롯 '{slot}'에 API 키가 없습니다. 설정 › 플러그인 › 설치됨에서 Git Commit Message Helper를 선택하고 시크릿에 해당 LLM API 키를 입력하세요.",
    llm_no_model: "'모델'이 비어 있습니다. 공급자 프리셋을 고르거나 설정 › 플러그인 › Git Commit Message Helper에서 '모델'을 입력하세요.",
    llm_no_base_url: "'기본 URL'이 비어 있습니다. 공급자 프리셋을 고르거나 설정 › 플러그인 › Git Commit Message Helper에서 '기본 URL'을 입력하세요.",
    llm_budget_spent: "{failure} ({attempts}번 시도한 뒤 작업 시간 예산을 모두 써서 중단했습니다)",
    llm_request_failed: "LLM 요청 실패(HTTP {status}): {detail}",
    llm_test_not_json: "LLM 테스트 응답이 JSON이 아닙니다: {error}: {body}",
    llm_not_json: "LLM 응답이 JSON이 아닙니다: {error}: {body}",
    llm_no_text: "LLM 응답에 텍스트가 없습니다: {body}",
};

pub fn strings(language: UiLanguage) -> &'static Strings {
    match language {
        UiLanguage::Zh => &ZH,
        UiLanguage::Ja => &JA,
        UiLanguage::Ko => &KO,
        UiLanguage::En => &EN,
    }
}

impl crate::prompt::QualityIssue {
    /// The notification text for one finding.
    pub fn text(self, strings: &Strings) -> &'static str {
        match self {
            crate::prompt::QualityIssue::VagueSubject => strings.quality_vague,
            crate::prompt::QualityIssue::FileNameOnly => strings.quality_filename,
            crate::prompt::QualityIssue::TooShortSubject => strings.quality_short,
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::UiLanguage;

    #[test]
    fn every_language_has_its_own_text() {
        assert_eq!(strings(UiLanguage::Zh).canceled, "已取消。");
        assert_eq!(strings(UiLanguage::En).canceled, "Cancelled.");
        assert_eq!(strings(UiLanguage::Ja).canceled, "中止しました。");
        assert_eq!(strings(UiLanguage::Ko).canceled, "취소했습니다.");
    }

    /// Chinese text names a setting, page, action or secret the way the
    /// Chinese settings page shows it, from `locales/zh-CN.json`.
    #[test]
    fn chinese_text_names_settings_as_the_settings_page_shows_them() {
        let catalog: std::collections::HashMap<String, String> =
            serde_json::from_str(include_str!("../locales/zh-CN.json")).unwrap();
        let zh = strings(UiLanguage::Zh);
        let cases: [(&str, &[&str]); 2] = [
            (zh.not_configured, &["LLM API key"]),
            (zh.template_help, &["Restore Default", "Default template"]),
        ];
        for (text, labels) in cases {
            for label in labels {
                let shown = catalog[*label].as_str();
                assert!(!text.contains(label), "{label:?} stays English in {text:?}");
                assert!(text.contains(shown), "{text:?} does not say {shown:?}");
            }
        }
        // The IDE's own path to the secrets, in the IDE's Chinese text.
        assert!(
            zh.not_configured.contains("设置 › 插件 › 已安装"),
            "{}",
            zh.not_configured
        );
    }

    /// Every language fills in the same `{name}` placeholders of an LLM
    /// error, so none drops the status, the server message or the body.
    #[test]
    fn llm_errors_keep_their_placeholders_in_every_language() {
        fn placeholders(text: &str) -> Vec<&str> {
            let mut found: Vec<&str> = text
                .match_indices('{')
                .filter_map(|(at, _)| text[at..].find('}').map(|end| &text[at..=at + end]))
                .collect();
            found.sort();
            found
        }
        let fields: [fn(&Strings) -> &'static str; 8] = [
            |s| s.llm_no_key,
            |s| s.llm_no_model,
            |s| s.llm_no_base_url,
            |s| s.llm_budget_spent,
            |s| s.llm_request_failed,
            |s| s.llm_test_not_json,
            |s| s.llm_not_json,
            |s| s.llm_no_text,
        ];
        for field in fields {
            let english = placeholders(field(strings(UiLanguage::En)));
            for language in [UiLanguage::Zh, UiLanguage::Ja, UiLanguage::Ko] {
                let text = field(strings(language));
                assert_eq!(placeholders(text), english, "{text}");
            }
        }
    }
}
