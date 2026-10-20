//! submit の結果の契約（R-SUBMIT / R-RESULT）。

/// submit で示す判断。終了コードは承認 0 / 変更要求 1（R-SUBMIT）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Approved,
    ChangesRequested,
}

impl Verdict {
    pub fn as_str(self) -> &'static str {
        match self {
            Verdict::Approved => "approved",
            Verdict::ChangesRequested => "changes_requested",
        }
    }

    pub fn parse(verdict: &str) -> Option<Self> {
        match verdict {
            "approved" => Some(Verdict::Approved),
            "changes_requested" => Some(Verdict::ChangesRequested),
            _ => None,
        }
    }

    pub fn exit_code(self) -> i32 {
        match self {
            Verdict::Approved => 0,
            Verdict::ChangesRequested => 1,
        }
    }
}
