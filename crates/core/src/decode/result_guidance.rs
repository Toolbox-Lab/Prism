//! Plain-English guidance for transaction and operation result codes.
//!
//! Diagnostic events explain Soroban failures in detail, but many failures
//! never produce them: a bad sequence number, a missing signature, an expired
//! transaction, an archived ledger entry. For those the only evidence is the
//! result code in the `TransactionResult` XDR. This module maps each of those
//! codes to a one-line summary, its likely causes and the usual fixes, so a
//! report always tells the user what went wrong and what to do next.
//!
//! Codes are matched by the PascalCase names the decoders already emit
//! (`TxBadSeq`, `Trapped`, `OpBadAuth`, ...).

use crate::types::report::{DiagnosticReport, RootCause, Severity, SuggestedFix};

/// Guidance for one result code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResultGuidance {
    /// One sentence describing what the code means.
    pub summary: &'static str,
    /// Likely causes, most likely first.
    pub causes: &'static [&'static str],
    /// Fixes as `(id, description, difficulty)`, best first.
    pub fixes: &'static [(&'static str, &'static str, &'static str)],
}

/// Look up guidance for a transaction-level or operation-level result code.
#[must_use]
pub fn guidance_for(code: &str) -> Option<ResultGuidance> {
    let guidance = match code {
        // Transaction-level codes (TransactionResultResult).
        "TxBadSeq" => ResultGuidance {
            summary: "The transaction's sequence number does not match the source account's next sequence number.",
            causes: &[
                "The transaction was built with a stale sequence number, for example after another transaction from the same account was submitted",
                "The same transaction, or another one with the same sequence number, was already submitted",
            ],
            fixes: &[(
                "refresh_sequence_number",
                "Reload the source account and rebuild the transaction with its current sequence number plus one",
                "easy",
            )],
        },
        "TxBadAuth" => ResultGuidance {
            summary: "The transaction does not have enough valid signatures, or was signed for a different network.",
            causes: &[
                "A required signer did not sign, or the signatures do not reach the account's threshold",
                "The transaction was signed with a different network passphrase, for example Testnet instead of Mainnet",
            ],
            fixes: &[
                (
                    "add_required_signatures",
                    "Sign with every key needed to reach the source account's thresholds",
                    "medium",
                ),
                (
                    "check_network_passphrase",
                    "Make sure the network passphrase used for signing matches the network you submit to",
                    "easy",
                ),
            ],
        },
        "TxBadAuthExtra" => ResultGuidance {
            summary: "The transaction carries signatures that are not needed.",
            causes: &["Extra or unrelated signatures were attached to the transaction"],
            fixes: &[(
                "remove_extra_signatures",
                "Remove signatures that do not belong to a required signer",
                "easy",
            )],
        },
        "TxInsufficientFee" => ResultGuidance {
            summary: "The fee offered is lower than the network currently requires.",
            causes: &[
                "The fee is below the minimum base fee or the current surge-pricing level",
                "For Soroban transactions, the resource fee does not cover the declared resources",
            ],
            fixes: &[(
                "raise_fee",
                "Raise the fee; for Soroban transactions, re-simulate and use the returned resource fee",
                "easy",
            )],
        },
        "TxInsufficientBalance" => ResultGuidance {
            summary: "Paying the fee would take the source account below its minimum XLM reserve.",
            causes: &["The source account's XLM balance is too low to cover the fee and its reserve"],
            fixes: &[(
                "fund_source_account",
                "Add XLM to the source account, or wrap the transaction in a fee bump paid by another account",
                "easy",
            )],
        },
        "TxNoAccount" => ResultGuidance {
            summary: "The source account does not exist on this network.",
            causes: &[
                "The account was never funded on this network",
                "The transaction is being submitted to the wrong network",
            ],
            fixes: &[(
                "create_source_account",
                "Fund the source account (on Testnet, with Friendbot) or submit to the network the account exists on",
                "easy",
            )],
        },
        "TxTooEarly" => ResultGuidance {
            summary: "The ledger closed before the transaction's minimum time bound.",
            causes: &["The transaction's minimum time is still in the future"],
            fixes: &[(
                "adjust_time_bounds",
                "Wait until the minimum time, or rebuild with time bounds that include the current time",
                "easy",
            )],
        },
        "TxTooLate" => ResultGuidance {
            summary: "The transaction expired: the ledger closed after its maximum time bound.",
            causes: &["The transaction waited too long before submission, or its timeout was too short"],
            fixes: &[(
                "rebuild_with_longer_timeout",
                "Rebuild, re-sign and resubmit the transaction with a longer timeout",
                "easy",
            )],
        },
        "TxMissingOperation" => ResultGuidance {
            summary: "The transaction contains no operations.",
            causes: &["The transaction was built without adding an operation"],
            fixes: &[(
                "add_operation",
                "Add at least one operation before signing",
                "easy",
            )],
        },
        "TxBadMinSeqAgeOrGap" => ResultGuidance {
            summary: "The transaction's minimum sequence age or ledger gap precondition was not met.",
            causes: &["The source account's sequence number changed too recently for these preconditions"],
            fixes: &[(
                "relax_sequence_preconditions",
                "Wait until the required age or ledger gap has passed, or remove these preconditions",
                "easy",
            )],
        },
        "TxMalformed" => ResultGuidance {
            summary: "The transaction's preconditions are malformed.",
            causes: &["Invalid time or ledger bounds, or an invalid combination of preconditions"],
            fixes: &[(
                "fix_preconditions",
                "Rebuild the transaction with valid preconditions",
                "medium",
            )],
        },
        "TxSorobanInvalid" => ResultGuidance {
            summary: "The transaction's Soroban data is invalid for this network.",
            causes: &[
                "The declared resources exceed the network's per-transaction limits",
                "The footprint or Soroban data does not match the operation",
            ],
            fixes: &[(
                "resimulate_transaction",
                "Re-simulate the transaction and use the Soroban data and resource fee it returns",
                "easy",
            )],
        },
        "TxNotSupported" | "OpNotSupported" => ResultGuidance {
            summary: "The network does not support this transaction or operation at its current protocol version.",
            causes: &["A feature that is not enabled at the network's protocol version was used"],
            fixes: &[(
                "check_protocol_version",
                "Check the network's protocol version and avoid features it does not support yet",
                "medium",
            )],
        },
        "TxBadSponsorship" => ResultGuidance {
            summary: "A sponsorship in the transaction was not properly closed.",
            causes: &["A BeginSponsoringFutureReserves operation has no matching EndSponsoringFutureReserves"],
            fixes: &[(
                "pair_sponsorship_operations",
                "Pair every BeginSponsoringFutureReserves with an EndSponsoringFutureReserves in the same transaction",
                "medium",
            )],
        },
        "TxInternalError" => ResultGuidance {
            summary: "The network hit an unexpected internal error while applying the transaction.",
            causes: &["An error inside the validator that the transaction contents did not cause"],
            fixes: &[(
                "retry_submission",
                "Retry the submission, and report the result to the network operators if it keeps happening",
                "easy",
            )],
        },

        // InvokeHostFunction operation results.
        "Trapped" => ResultGuidance {
            summary: "The contract call failed during execution: the contract panicked or returned an error.",
            causes: &[
                "The contract returned an error or panicked, for example on a failed assertion or a contract-defined error",
                "An authorization check inside the call failed",
            ],
            fixes: &[
                (
                    "inspect_diagnostic_events",
                    "Decode the transaction together with its result meta, or by hash, to see the diagnostic events that show which call failed and why",
                    "easy",
                ),
                (
                    "simulate_before_submit",
                    "Simulate the call before submitting; simulation reports the same error without charging fees",
                    "easy",
                ),
            ],
        },
        "ResourceLimitExceeded" => ResultGuidance {
            summary: "Execution used more resources (CPU, memory, ledger reads or writes) than the transaction declared.",
            causes: &[
                "The declared resource limits were too low for this call",
                "The call does more work than expected, for example an unbounded loop over storage",
            ],
            fixes: &[
                (
                    "resimulate_resources",
                    "Re-simulate the transaction and use the resource limits it returns, with a safety margin",
                    "easy",
                ),
                (
                    "reduce_work_per_call",
                    "Reduce the work done in a single call, for example by paginating loops",
                    "medium",
                ),
            ],
        },
        "EntryArchived" => ResultGuidance {
            summary: "A ledger entry the call needs has been archived and must be restored first.",
            causes: &["The time-to-live of a contract data or contract code entry in the footprint ran out"],
            fixes: &[
                (
                    "restore_footprint",
                    "Submit a RestoreFootprint operation for the archived entries, then retry the call",
                    "medium",
                ),
                (
                    "extend_entry_ttl",
                    "Extend the time-to-live of long-lived entries so they do not expire again",
                    "medium",
                ),
            ],
        },
        "InsufficientRefundableFee" => ResultGuidance {
            summary: "The refundable part of the resource fee did not cover the rent and events this call produced.",
            causes: &["The resource fee was set too low for the rent or events this call produced"],
            fixes: &[(
                "resimulate_resource_fee",
                "Re-simulate the transaction and use the resource fee it returns",
                "easy",
            )],
        },
        "Malformed" => ResultGuidance {
            summary: "The host function call is malformed.",
            causes: &["The arguments, footprint or Soroban data for the call are invalid"],
            fixes: &[(
                "rebuild_invocation",
                "Rebuild the invocation and re-simulate it before submitting",
                "medium",
            )],
        },

        // Operation-level codes that apply to any operation (OperationResult).
        "OpBadAuth" => ResultGuidance {
            summary: "An operation's source account did not authorize it.",
            causes: &["The operation has its own source account, and that account did not sign"],
            fixes: &[(
                "sign_operation_source",
                "Add a signature from the operation's source account",
                "medium",
            )],
        },
        "OpNoAccount" => ResultGuidance {
            summary: "An operation's source account does not exist.",
            causes: &["The operation-level source account was never funded on this network"],
            fixes: &[(
                "fund_operation_source",
                "Fund the operation's source account, or remove the per-operation source account",
                "easy",
            )],
        },
        "OpTooManySubentries" => ResultGuidance {
            summary: "The account has reached the maximum number of subentries (1,000).",
            causes: &["The account holds too many trustlines, offers, signers or data entries"],
            fixes: &[(
                "remove_subentries",
                "Remove unused trustlines, offers, signers or data entries from the account",
                "medium",
            )],
        },
        "OpExceededWorkLimit" => ResultGuidance {
            summary: "The operation did more work than the network allows in one operation.",
            causes: &["The operation touched too many ledger entries, for example a large offer crossing"],
            fixes: &[(
                "split_operation",
                "Split the work across several operations or transactions",
                "medium",
            )],
        },
        "OpTooManySponsoring" => ResultGuidance {
            summary: "The sponsoring account already sponsors the maximum number of entries.",
            causes: &["The account sponsors too many ledger entries"],
            fixes: &[(
                "reduce_sponsorships",
                "Reduce the number of entries this account sponsors, or use another sponsor",
                "medium",
            )],
        },
        _ => return None,
    };
    Some(guidance)
}

/// Add root causes and suggested fixes for the report's result code.
///
/// Successful (`Info`) reports are left alone. Causes and fixes that are
/// already present, for example from diagnostic events, are kept and not
/// duplicated, and they stay first because they are more specific.
pub fn add_guidance(report: &mut DiagnosticReport) {
    if report.severity == Severity::Info {
        return;
    }
    let Some(guidance) = guidance_for(&report.error_name) else {
        return;
    };

    for (index, cause) in guidance.causes.iter().enumerate() {
        if report
            .root_causes
            .iter()
            .any(|existing| existing.description == *cause)
        {
            continue;
        }
        report.root_causes.push(RootCause {
            description: (*cause).to_string(),
            likelihood: if index == 0 { "high" } else { "medium" }.to_string(),
        });
    }

    for (id, description, difficulty) in guidance.fixes {
        if report
            .suggested_fixes
            .iter()
            .any(|existing| existing.id == *id)
        {
            continue;
        }
        report.suggested_fixes.push(SuggestedFix {
            description: (*description).to_string(),
            difficulty: (*difficulty).to_string(),
            requires_upgrade: false,
            example: None,
            id: (*id).to_string(),
            remedy_code: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_CODES: &[&str] = &[
        "TxBadSeq",
        "TxBadAuth",
        "TxBadAuthExtra",
        "TxInsufficientFee",
        "TxInsufficientBalance",
        "TxNoAccount",
        "TxTooEarly",
        "TxTooLate",
        "TxMissingOperation",
        "TxBadMinSeqAgeOrGap",
        "TxMalformed",
        "TxSorobanInvalid",
        "TxNotSupported",
        "TxBadSponsorship",
        "TxInternalError",
        "Trapped",
        "ResourceLimitExceeded",
        "EntryArchived",
        "InsufficientRefundableFee",
        "Malformed",
        "OpBadAuth",
        "OpNoAccount",
        "OpNotSupported",
        "OpTooManySubentries",
        "OpExceededWorkLimit",
        "OpTooManySponsoring",
    ];

    #[test]
    fn every_known_code_has_complete_guidance() {
        for code in ALL_CODES {
            let guidance = guidance_for(code).unwrap_or_else(|| panic!("no guidance for {code}"));
            assert!(!guidance.summary.is_empty(), "{code}: empty summary");
            assert!(!guidance.causes.is_empty(), "{code}: no causes");
            assert!(!guidance.fixes.is_empty(), "{code}: no fixes");
            for (id, description, difficulty) in guidance.fixes {
                assert!(
                    !id.is_empty() && !description.is_empty(),
                    "{code}: blank fix"
                );
                assert!(
                    matches!(*difficulty, "easy" | "medium" | "hard"),
                    "{code}: unexpected difficulty {difficulty}"
                );
            }
        }
    }

    #[test]
    fn unknown_and_success_codes_have_no_guidance() {
        assert!(guidance_for("Success").is_none());
        assert!(guidance_for("TxSuccess").is_none());
        assert!(guidance_for("SomethingElse").is_none());
    }

    #[test]
    fn add_guidance_fills_causes_and_fixes_for_failures() {
        let mut report = DiagnosticReport::new("Transaction", 0, "TxBadSeq", "failed");
        report.severity = Severity::Error;

        add_guidance(&mut report);

        assert_eq!(report.root_causes.len(), 2);
        assert_eq!(report.root_causes[0].likelihood, "high");
        assert_eq!(report.root_causes[1].likelihood, "medium");
        assert_eq!(report.suggested_fixes.len(), 1);
        assert_eq!(report.suggested_fixes[0].id, "refresh_sequence_number");
    }

    #[test]
    fn add_guidance_keeps_existing_entries_first_and_does_not_duplicate() {
        let mut report = DiagnosticReport::new("Soroban", 0, "Trapped", "failed");
        report.severity = Severity::Error;
        report.root_causes.push(RootCause {
            description: "Deepest diagnostic error occurred at call depth 1".to_string(),
            likelihood: "high".to_string(),
        });

        add_guidance(&mut report);
        add_guidance(&mut report);

        assert!(report.root_causes[0]
            .description
            .starts_with("Deepest diagnostic error"));
        assert_eq!(report.root_causes.len(), 3);
        assert_eq!(report.suggested_fixes.len(), 2);
    }

    #[test]
    fn add_guidance_skips_successful_reports() {
        let mut report = DiagnosticReport::new("Transaction", 0, "TxBadSeq", "ok");
        report.severity = Severity::Info;

        add_guidance(&mut report);

        assert!(report.root_causes.is_empty());
        assert!(report.suggested_fixes.is_empty());
    }
}
