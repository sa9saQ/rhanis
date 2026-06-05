//! コスト追跡と予算ハードキャップ。
//!
//! BYOK（ユーザーが自分の OpenAI キーを使う方式）では、高額課金を負うのは
//! ユーザー自身になる。音声リアルタイム API は高価（GPT-Realtime-2 は
//! 1 分あたり概ね $0.1〜0.5）なので、知らぬ間に月数万円という事故を防ぐため、
//! usage（トークン数）から料金を換算し、月次累計が上限に達したら
//! セッション開始をブロックする（fail-closed）。
//!
//! 制限の ON/OFF と金額の判断はユーザーに委ねる（`BudgetConfig::enabled`）。
//! OpenAI ダッシュボード側の上限とは独立した、アプリ内で完結する安全網。
//!
//! ## 設計上の不変条件（R-C / Codex レビュー反映）
//! - **金額は整数ナノドル**で扱う。f64 の丸め誤差や NaN/Inf による fail-open
//!   （安全装置の無効化）を構造的に排除する。
//! - **月リセットは月が前進した時のみ**。過去月・無効月の入力では累計をリセット
//!   せず、リセット悪用による無料化を防ぐ。
//! - 算術はすべて `saturating_*`。オーバーフローしても panic せず上限側に張り付く
//!   （= 予算超過と判定され、fail-closed）。
//!
//! ## 呼び出し側（session_manager / settings UI）の責務
//! - session_manager は開始時の `can_start_session()` に加え、usage 受信ごとに
//!   `is_over_budget()` を確認し、超過したら進行中セッションを即停止すること
//!   （開始後の野放し課金を防ぐ）。複数同時セッションの原子的予約は M2 で導入。
//! - `enabled = false` は「ユーザーが明示的に無制限を選んだ」状態。初回オンボー
//!   ディングで「上限を設定する / 明示的に無制限」を必須選択にし、未設定のまま
//!   高額課金が起きないようにすること（settings UI 層の責務）。
//
// --- 課金安全 hook bypass note（本モジュールが該当しない根拠） ---
// idempotency_key N/A / FOR UPDATE N/A / transaction N/A:
//   本モジュールは料金の"計算"のみで、Stripe 等の決済・DB 書き込み・残高更新を行わない。
// 正数検証 N/A:
//   トークン数・料金・累計・上限はすべて u64（負値・NaN・Inf を型レベルで排除）。
//   ユーザー入力の USD は usd_to_nanodollars() で finite && 非負を検証してから u64 化する。

use serde::{Deserialize, Serialize};

/// 1 USD = 1,000,000,000 ナノドル。金額はこの整数単位で保持する。
pub const NANODOLLARS_PER_USD: u64 = 1_000_000_000;

/// 料金単価（ナノドル / トークン）。USD/100 万トークンを整数化したもの。
///
/// 例: 音声入力 $32 / 1M tokens = 32 USD / 1e6 tokens
///   = 32 * 1e9 nanodollars / 1e6 tokens = 32_000 nanodollars/token。
/// 出典: OpenAI GPT-Realtime-2 pricing 2026-05。単価が変わったらここだけ直す。
pub mod pricing {
    pub const AUDIO_INPUT_PER_TOKEN: u64 = 32_000;
    pub const AUDIO_OUTPUT_PER_TOKEN: u64 = 64_000;
    pub const TEXT_INPUT_PER_TOKEN: u64 = 4_000;
    pub const TEXT_OUTPUT_PER_TOKEN: u64 = 24_000;
    /// キャッシュ済み入力は大幅割引（繰り返し送られる system prompt 等）。$0.40/1M = 400/token。
    pub const CACHED_INPUT_PER_TOKEN: u64 = 400;
}

/// USD を nanodollars に変換（UI 入力用）。
/// 非有限（NaN/Inf）・負値・u64 オーバーフロー（巨大値が `u64::MAX` に丸め込まれて
/// 実質無制限化するのを防ぐ）は `None`（不正値を u64 に通さない fail-closed）。
pub fn usd_to_nanodollars(usd: f64) -> Option<u64> {
    if !usd.is_finite() || usd < 0.0 {
        return None;
    }
    let nano = usd * NANODOLLARS_PER_USD as f64;
    // u64::MAX ≈ 1.8e19 nano = $1.8e10。それを超える USD は不正入力扱い。
    if nano > u64::MAX as f64 {
        return None;
    }
    Some(nano.round() as u64)
}

/// nanodollars を表示用 USD に変換（表示のみ。判定・比較には使わない）。
pub fn nanodollars_to_usd(nano: u64) -> f64 {
    nano as f64 / NANODOLLARS_PER_USD as f64
}

/// 1 レスポンス分のトークン使用量。Realtime API の `usage` イベント由来。
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Usage {
    pub audio_input_tokens: u64,
    pub audio_output_tokens: u64,
    pub text_input_tokens: u64,
    pub text_output_tokens: u64,
    pub cached_input_tokens: u64,
}

impl Usage {
    /// この usage 分の料金（ナノドル）。
    /// オーバーフローは saturating（上限張り付き = 予算超過側 = fail-closed）。
    pub fn cost_nanodollars(&self) -> u64 {
        use pricing::*;
        let mul = |t: u64, r: u64| t.saturating_mul(r);
        mul(self.audio_input_tokens, AUDIO_INPUT_PER_TOKEN)
            .saturating_add(mul(self.audio_output_tokens, AUDIO_OUTPUT_PER_TOKEN))
            .saturating_add(mul(self.text_input_tokens, TEXT_INPUT_PER_TOKEN))
            .saturating_add(mul(self.text_output_tokens, TEXT_OUTPUT_PER_TOKEN))
            .saturating_add(mul(self.cached_input_tokens, CACHED_INPUT_PER_TOKEN))
    }

    /// 表示用の USD 換算（UI 表示専用、判定には使わない）。
    pub fn cost_usd(&self) -> f64 {
        nanodollars_to_usd(self.cost_nanodollars())
    }
}

/// 予算設定。`enabled = false` なら無制限（ユーザーが明示設定するまで縛らない）。
/// 金額は nanodollars（整数）で保持し、NaN/Inf を型レベルで排除する。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct BudgetConfig {
    pub enabled: bool,
    pub monthly_limit_nanodollars: u64,
}

impl Default for BudgetConfig {
    fn default() -> Self {
        // 既定は OFF。ユーザーが上限額を入れて初めて制限が効く（オンボーディング必須）。
        Self {
            enabled: false,
            monthly_limit_nanodollars: 0,
        }
    }
}

impl BudgetConfig {
    /// `total_nanodollars` がこの予算を超過しているか。`enabled = false`
    /// （ユーザーが明示的に無制限を選択）は常に false。上限ちょうども超過扱い
    /// （fail-closed 寄り）。
    ///
    /// `total` には **世代横断の永続合計**（= 単一セッションの local 合計以上）を
    /// 渡せる。session_manager は usage 受信ごとに共有コストスナップショットを
    /// 読み戻してこの判定を行うため、stop→start handover で古いセッションが
    /// 遅延 usage を処理し続けても、新しいセッションが古い local baseline で
    /// fail-open することを防ぐ（koe-ixt 機序4）。超過判定の単一の真実源であり、
    /// [`CostTracker::is_over_budget`] はこれに委譲する。
    pub fn is_over(&self, total_nanodollars: u64) -> bool {
        self.enabled && total_nanodollars >= self.monthly_limit_nanodollars
    }

    /// 任意の `total_nanodollars` に対する上限までの残額（ナノドル、負にはならない）。
    /// `enabled = false`（無制限）なら `None`。
    ///
    /// 「残額」の単一の真実源。[`CostTracker::remaining_nanodollars`] と
    /// [`CostSnapshot::new`] の両方がこれに委譲するので、tracker の残額と UI に
    /// 出す残額が drift しない（`is_over` を共有述語化したのと同じ理由、git.md の
    /// dual-validator drift 教訓）。`is_over` と同様に世代横断の永続合計を渡せる。
    pub fn remaining_nanodollars(&self, total_nanodollars: u64) -> Option<u64> {
        if !self.enabled {
            return None;
        }
        Some(
            self.monthly_limit_nanodollars
                .saturating_sub(total_nanodollars),
        )
    }
}

/// 月次のコスト累計 + 予算判定。
///
/// `current_month` は `YYYYMM`（例: 2026 年 5 月 = `202605`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostTracker {
    pub config: BudgetConfig,
    pub current_month: u32,
    pub month_total_nanodollars: u64,
}

impl CostTracker {
    pub fn new(config: BudgetConfig, current_month: u32) -> Self {
        Self {
            config,
            current_month,
            month_total_nanodollars: 0,
        }
    }

    /// `YYYYMM` の妥当性（月部分 01-12、年 2000-9999）。
    fn is_valid_month(month: u32) -> bool {
        let mm = month % 100;
        let yyyy = month / 100;
        (1..=12).contains(&mm) && (2000..=9999).contains(&yyyy)
    }

    /// usage を累計に加算し、加算後の月次累計（ナノドル）を返す。
    ///
    /// 月リセットは **月が前進（`month > current_month`）した時のみ**。
    /// 過去月・無効月の入力では累計をリセットせず現在の月に計上する
    /// （`202605 -> 202604 -> 202605` のようなリセット悪用による無料化を防ぐ）。
    pub fn add_usage(&mut self, usage: &Usage, month: u32) -> u64 {
        if Self::is_valid_month(month) && month > self.current_month {
            self.current_month = month;
            self.month_total_nanodollars = 0;
        }
        self.month_total_nanodollars = self
            .month_total_nanodollars
            .saturating_add(usage.cost_nanodollars());
        self.month_total_nanodollars
    }

    /// 予算超過か。`enabled = false` なら常に false（無制限）。
    /// 上限ちょうどに達した時点で「超過」とみなす（fail-closed 寄り）。
    /// 判定本体は [`BudgetConfig::is_over`] に委譲する（session_manager が
    /// 世代横断の永続合計で判定する経路と同一の述語を使うため、koe-ixt）。
    pub fn is_over_budget(&self) -> bool {
        self.config.is_over(self.month_total_nanodollars)
    }

    /// 新しいセッションを開始してよいか。予算超過なら false（fail-closed）。
    pub fn can_start_session(&self) -> bool {
        !self.is_over_budget()
    }

    /// 上限までの残額（ナノドル、負にはならない）。`enabled = false` なら None（無制限）。
    /// 残額の述語は [`BudgetConfig::remaining_nanodollars`] に委譲する（CostSnapshot が
    /// 世代横断の永続合計で残額を出す経路と同一の述語を使い、drift を防ぐ）。
    pub fn remaining_nanodollars(&self) -> Option<u64> {
        self.config
            .remaining_nanodollars(self.month_total_nanodollars)
    }

    /// 上限までの残額（表示用 USD）。
    pub fn remaining_usd(&self) -> Option<f64> {
        self.remaining_nanodollars().map(nanodollars_to_usd)
    }
}

/// ある時点の「今月の使用額 + 予算状態」を frontend に渡す単一の DTO（koe-9xi）。
///
/// pull（`get_cost_snapshot` コマンド、起動直後の確定値）と push（session_manager の
/// `cost-update` emit、会話中のライブ更新）が **この同一コンストラクタ**で組むので、
/// 二経路で表示が drift しない。
///
/// ## 不変条件（崩すと課金事故 = 信用崩壊）
/// - `over_budget` は **ここ Rust 側で u64 比較**（[`BudgetConfig::is_over`] = `>=`、
///   上限ちょうども超過）で確定し bool で渡す。`used_usd` / `remaining_usd` は
///   **表示専用の f64**。frontend はこの f64 から超過を再計算してはならない
///   （判定は u64、表示のみ f64）。
/// - `used_nanodollars` は **権威ある整数合計**（pull は `load_cost_snapshot` の値、
///   push は additive ledger の戻り値 = 世代横断合計）。session-local tracker の
///   total を表示権威にしない（koe-ixt）。
/// - payload は **数値と bool のみ**。API キー・パス・PII を一切含めない。
/// - `sequence` は共有 [`crate::events::SequenceCounter`] から採番した単調増加値。
///   frontend (costStore) は `sequence` が小さい古い snapshot を捨てるので、古い
///   低額 snapshot が新しい超過 snapshot を上書きして停止 UI を隠す fail-open 表示
///   が起きない（session-status / activityStore と同じ stale guard 思想）。
///
/// f64 を含むため `Eq` は導出しない（`PartialEq` のみ）。`used_usd` / `remaining_usd`
/// は `u64 / 1e9` なので NaN/Inf になり得ず、serde_json で `null` に化けることはない。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct CostSnapshot {
    /// 会計上の月（`YYYYMM`）。
    pub month: u32,
    /// 今月の使用額（ナノドル）— 権威ある整数合計。
    pub used_nanodollars: u64,
    /// 上限（ナノドル）。予算無効（無制限）なら `None`。
    pub limit_nanodollars: Option<u64>,
    /// ハードキャップが有効か。
    pub enabled: bool,
    /// 今月の使用額が上限に達した / 超えたか（u64 `>=`）。無効なら常に false。
    /// **Rust 側で確定**し、frontend は f64 から再計算しない。
    pub over_budget: bool,
    /// 共有カウンタ由来の単調増加 sequence。古い snapshot を捨てる stale guard 用。
    pub sequence: u64,
    /// 表示専用 USD（使用額）。比較・判定には使わない。
    pub used_usd: f64,
    /// 表示専用 USD（上限までの残額）。無制限なら `None`。
    pub remaining_usd: Option<f64>,
}

impl CostSnapshot {
    /// `(month, used_nanodollars, BudgetConfig, sequence)` から snapshot を組む純粋関数。
    /// `over_budget` / `remaining` は [`BudgetConfig`] の共有述語に委譲し、f64 は
    /// 表示用に [`nanodollars_to_usd`] で換算するだけ（判定には混ぜない）。
    pub fn new(
        month: u32,
        used_nanodollars: u64,
        config: &BudgetConfig,
        sequence: u64,
    ) -> Self {
        let over_budget = config.is_over(used_nanodollars);
        let remaining_nanodollars = config.remaining_nanodollars(used_nanodollars);
        let limit_nanodollars = if config.enabled {
            Some(config.monthly_limit_nanodollars)
        } else {
            None
        };
        Self {
            month,
            used_nanodollars,
            limit_nanodollars,
            enabled: config.enabled,
            over_budget,
            sequence,
            used_usd: nanodollars_to_usd(used_nanodollars),
            remaining_usd: remaining_nanodollars.map(nanodollars_to_usd),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const USD: u64 = NANODOLLARS_PER_USD;

    #[test]
    fn cost_single_meter() {
        // 音声入力 100 万トークン = ちょうど $32。
        let u = Usage {
            audio_input_tokens: 1_000_000,
            ..Default::default()
        };
        assert_eq!(u.cost_nanodollars(), 32 * USD);
    }

    #[test]
    fn cost_all_meters_combined() {
        let u = Usage {
            audio_input_tokens: 1_000_000,  // $32
            audio_output_tokens: 1_000_000, // $64
            text_input_tokens: 1_000_000,   // $4
            text_output_tokens: 1_000_000,  // $24
            cached_input_tokens: 1_000_000, // $0.40
        };
        // $124.40 = 124_400_000_000 nanodollars（整数なので誤差なし）
        assert_eq!(u.cost_nanodollars(), 124_400_000_000);
    }

    #[test]
    fn cost_one_minute_voice_estimate() {
        // 1 分会話の概算: ユーザー音声 600 tokens + アシスタント音声 1200 tokens。
        let u = Usage {
            audio_input_tokens: 600,
            audio_output_tokens: 1200,
            ..Default::default()
        };
        // 600*32_000 + 1200*64_000 = 19_200_000 + 76_800_000 = 96_000_000 ($0.096)
        assert_eq!(u.cost_nanodollars(), 96_000_000);
    }

    #[test]
    fn empty_usage_is_free() {
        assert_eq!(Usage::default().cost_nanodollars(), 0);
    }

    #[test]
    fn cost_saturates_instead_of_overflow() {
        // 極端なトークン数でも panic せず u64::MAX に張り付く（fail-closed）。
        let u = Usage {
            audio_output_tokens: u64::MAX,
            ..Default::default()
        };
        assert_eq!(u.cost_nanodollars(), u64::MAX);
    }

    #[test]
    fn usd_conversion_rejects_non_finite_and_negative() {
        assert_eq!(usd_to_nanodollars(f64::NAN), None);
        assert_eq!(usd_to_nanodollars(f64::INFINITY), None);
        assert_eq!(usd_to_nanodollars(-1.0), None);
        assert_eq!(usd_to_nanodollars(10.0), Some(10 * USD));
    }

    #[test]
    fn usd_conversion_rejects_overflow() {
        // R-C round 2: 巨大有限値が `u64::MAX` に丸め込まれて実質無制限化するのを防ぐ。
        assert_eq!(usd_to_nanodollars(1.0e20), None);
        assert_eq!(usd_to_nanodollars(1.0e30), None);
        assert_eq!(usd_to_nanodollars(f64::MAX), None);
        // 実用範囲（数百〜数百万 USD）は通る。
        assert_eq!(usd_to_nanodollars(1_000_000.0), Some(1_000_000 * USD));
    }

    #[test]
    fn add_usage_accumulates_within_month() {
        let mut t = CostTracker::new(BudgetConfig::default(), 202605);
        let u = Usage {
            audio_input_tokens: 1_000_000,
            ..Default::default()
        };
        assert_eq!(t.add_usage(&u, 202605), 32 * USD);
        assert_eq!(t.add_usage(&u, 202605), 64 * USD);
        assert_eq!(t.current_month, 202605);
    }

    #[test]
    fn month_forward_resets_total() {
        let mut t = CostTracker::new(BudgetConfig::default(), 202605);
        let u = Usage {
            audio_input_tokens: 1_000_000,
            ..Default::default()
        };
        t.add_usage(&u, 202605);
        // 翌月（前進）になったら累計はリセットされ、その月の最初の usage だけが残る。
        assert_eq!(t.add_usage(&u, 202606), 32 * USD);
        assert_eq!(t.current_month, 202606);
    }

    #[test]
    fn past_month_does_not_reset_budget_bypass() {
        // P1 修正: 202605 -> 202604（過去月）-> 202605 のリセット悪用を防ぐ。
        let mut t = CostTracker::new(BudgetConfig::default(), 202605);
        let u = Usage {
            audio_input_tokens: 500_000, // $16
            ..Default::default()
        };
        t.add_usage(&u, 202605); // 累計 $16
        // 過去月を渡してもリセットされず、現在の月に加算される。
        assert_eq!(t.add_usage(&u, 202604), 32 * USD);
        assert_eq!(t.current_month, 202605); // 月は遡らない
    }

    #[test]
    fn invalid_month_does_not_reset() {
        let mut t = CostTracker::new(BudgetConfig::default(), 202605);
        let u = Usage {
            audio_input_tokens: 500_000, // $16
            ..Default::default()
        };
        t.add_usage(&u, 202605);
        // 無効月（13 月、年 0 等）ではリセットせず加算（fail-closed）。
        assert_eq!(t.add_usage(&u, 202613), 32 * USD);
        assert_eq!(t.add_usage(&u, 0), 48 * USD);
        assert_eq!(t.current_month, 202605);
    }

    #[test]
    fn disabled_budget_never_blocks() {
        let mut t = CostTracker::new(
            BudgetConfig {
                enabled: false,
                monthly_limit_nanodollars: 1 * USD,
            },
            202605,
        );
        let big = Usage {
            audio_output_tokens: 100_000_000, // $6400
            ..Default::default()
        };
        t.add_usage(&big, 202605);
        assert!(!t.is_over_budget());
        assert!(t.can_start_session());
        assert_eq!(t.remaining_nanodollars(), None);
        assert_eq!(t.remaining_usd(), None);
    }

    #[test]
    fn over_budget_blocks_at_or_above_limit() {
        let mut t = CostTracker::new(
            BudgetConfig {
                enabled: true,
                monthly_limit_nanodollars: 32 * USD,
            },
            202605,
        );
        let half = Usage {
            audio_input_tokens: 500_000, // $16
            ..Default::default()
        };
        // 上限未満なら開始可。
        t.add_usage(&half, 202605); // 累計 $16
        assert!(!t.is_over_budget());
        assert!(t.can_start_session());
        assert_eq!(t.remaining_nanodollars(), Some(16 * USD));

        // 上限ちょうど ($32) に達したらブロック（fail-closed、整数なので誤差なし）。
        t.add_usage(&half, 202605); // 累計 $32
        assert!(t.is_over_budget());
        assert!(!t.can_start_session());
        assert_eq!(t.remaining_nanodollars(), Some(0));
    }

    #[test]
    fn remaining_clamps_to_zero_when_exceeded() {
        let mut t = CostTracker::new(
            BudgetConfig {
                enabled: true,
                monthly_limit_nanodollars: 10 * USD,
            },
            202605,
        );
        let over = Usage {
            audio_input_tokens: 1_000_000, // $32 >> $10
            ..Default::default()
        };
        t.add_usage(&over, 202605);
        assert_eq!(t.remaining_nanodollars(), Some(0)); // 負にならず 0
        assert!(t.is_over_budget());
    }

    #[test]
    fn budget_config_is_over_gates_on_arbitrary_total_fail_closed() {
        // koe-ixt: the session loop gates the budget on the GLOBAL persisted total
        // (>= a single session's local total) read back from the shared cost
        // snapshot, so the predicate must accept an arbitrary total with the same
        // fail-closed semantics as is_over_budget.
        let on = BudgetConfig {
            enabled: true,
            monthly_limit_nanodollars: 32 * USD,
        };
        assert!(!on.is_over(31 * USD));
        assert!(on.is_over(32 * USD), "at-limit is over (fail-closed)");
        assert!(on.is_over(100 * USD));
        // Disabled = unlimited: never over, even for a saturated total.
        let off = BudgetConfig {
            enabled: false,
            monthly_limit_nanodollars: 32 * USD,
        };
        assert!(!off.is_over(u64::MAX));
        // is_over_budget delegates to is_over on the tracker's own running total,
        // so the two predicates can never disagree for the local total.
        let mut t = CostTracker::new(on, 202605);
        t.month_total_nanodollars = 32 * USD;
        assert!(t.is_over_budget());
        assert_eq!(t.is_over_budget(), on.is_over(t.month_total_nanodollars));
    }

    // ---- BudgetConfig::remaining_nanodollars (shared predicate) -----------

    #[test]
    fn budget_config_remaining_matches_tracker_remaining() {
        // The shared predicate must agree with the tracker's own remaining for the
        // tracker's running total (no drift between the UI's remaining and the
        // tracker's), and clamp to 0 / None at the edges.
        let on = BudgetConfig {
            enabled: true,
            monthly_limit_nanodollars: 32 * USD,
        };
        for used in [0u64, 16 * USD, 32 * USD, 100 * USD] {
            let mut t = CostTracker::new(on, 202605);
            t.month_total_nanodollars = used;
            assert_eq!(t.remaining_nanodollars(), on.remaining_nanodollars(used));
        }
        // At/over the limit clamps to Some(0), never negative.
        assert_eq!(on.remaining_nanodollars(40 * USD), Some(0));
        assert_eq!(on.remaining_nanodollars(32 * USD), Some(0));
        // Disabled = unlimited → None, even for a saturated total.
        let off = BudgetConfig {
            enabled: false,
            monthly_limit_nanodollars: 32 * USD,
        };
        assert_eq!(off.remaining_nanodollars(u64::MAX), None);
    }

    // ---- CostSnapshot -----------------------------------------------------

    #[test]
    fn cost_snapshot_under_budget() {
        let cfg = BudgetConfig {
            enabled: true,
            monthly_limit_nanodollars: 32 * USD,
        };
        let s = CostSnapshot::new(202605, 16 * USD, &cfg, 7);
        assert_eq!(s.month, 202605);
        assert_eq!(s.used_nanodollars, 16 * USD);
        assert_eq!(s.limit_nanodollars, Some(32 * USD));
        assert!(s.enabled);
        assert!(!s.over_budget);
        assert_eq!(s.sequence, 7);
        assert_eq!(s.used_usd, 16.0);
        assert_eq!(s.remaining_usd, Some(16.0));
    }

    #[test]
    fn cost_snapshot_over_budget_at_exactly_limit() {
        // 上限ちょうど ($32 of $32) は超過扱い（fail-closed、u64 `>=`）。
        let cfg = BudgetConfig {
            enabled: true,
            monthly_limit_nanodollars: 32 * USD,
        };
        let s = CostSnapshot::new(202605, 32 * USD, &cfg, 1);
        assert!(s.over_budget, "at-limit is over_budget (>=)");
        assert_eq!(s.remaining_usd, Some(0.0));
    }

    #[test]
    fn cost_snapshot_over_budget_above_limit_remaining_clamps() {
        let cfg = BudgetConfig {
            enabled: true,
            monthly_limit_nanodollars: 10 * USD,
        };
        let s = CostSnapshot::new(202605, 32 * USD, &cfg, 2);
        assert!(s.over_budget);
        // remaining saturates at 0, never negative.
        assert_eq!(s.remaining_usd, Some(0.0));
        assert_eq!(s.used_usd, 32.0);
    }

    #[test]
    fn cost_snapshot_unlimited_is_never_over_and_hides_limit() {
        // enabled=false（明示的に無制限）: 巨額でも over_budget=false、limit/remaining は None。
        let cfg = BudgetConfig {
            enabled: false,
            monthly_limit_nanodollars: 0,
        };
        let s = CostSnapshot::new(202605, 6_400 * USD, &cfg, 3);
        assert!(!s.enabled);
        assert!(!s.over_budget);
        assert_eq!(s.limit_nanodollars, None);
        assert_eq!(s.remaining_usd, None);
        assert_eq!(s.used_usd, 6_400.0);
    }

    #[test]
    fn cost_snapshot_over_budget_judged_in_u64_not_f64() {
        // over_budget は u64 比較で確定する（f64 表示と独立）。over_budget は
        // is_over に、used_usd は nanodollars_to_usd に厳密一致する。
        let cfg = BudgetConfig {
            enabled: true,
            monthly_limit_nanodollars: 32 * USD,
        };
        for used in [0u64, 1, 31 * USD, 32 * USD - 1, 32 * USD, u64::MAX] {
            let s = CostSnapshot::new(202605, used, &cfg, 0);
            assert_eq!(s.over_budget, cfg.is_over(used));
            assert_eq!(s.used_usd, nanodollars_to_usd(used));
        }
    }

    #[test]
    fn cost_snapshot_sequence_is_carried_verbatim() {
        let cfg = BudgetConfig::default();
        assert_eq!(CostSnapshot::new(202605, 0, &cfg, 0).sequence, 0);
        assert_eq!(CostSnapshot::new(202605, 0, &cfg, 999).sequence, 999);
        assert_eq!(
            CostSnapshot::new(202605, 0, &cfg, u64::MAX).sequence,
            u64::MAX
        );
    }

    #[test]
    fn cost_snapshot_display_usd_is_finite_never_nan_or_inf() {
        // used_usd / remaining_usd は u64/1e9 なので常に有限（serde_json で null 化しない）。
        let cfg = BudgetConfig {
            enabled: true,
            monthly_limit_nanodollars: u64::MAX,
        };
        let s = CostSnapshot::new(202605, u64::MAX, &cfg, 0);
        assert!(s.used_usd.is_finite());
        assert!(s.remaining_usd.unwrap().is_finite());
    }
}
