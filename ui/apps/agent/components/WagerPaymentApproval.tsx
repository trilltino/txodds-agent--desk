import { useEffect, useRef, useState } from 'react'
import type { SolanaPayIntent } from '../../../types'
import { createSolanaPayIntentNative, verifySolanaPayIntentNative } from '../../../desktop/transport'
import type { Wager } from '../../../core/agent/types'

// WagerPaymentApproval — the wallet-approval-before-settlement step
// (rig-venice ROADMAP.md Phase 7, item 3). This is the one place in the
// whole roadmap that cannot be verified without a live Phantom wallet
// session: the backend commands (create/verify_solana_pay_intent) are real
// and tested, but whether a human can actually scan/open this URL and land
// a confirmable transaction has not been exercised end-to-end here.
//
// Nothing in this component can move funds. It only requests a transfer
// (a `solana:` Transfer Request URL) that the user's own wallet must open
// and sign; `verify_solana_pay_intent` only ever reads chain state, never
// writes it.

const MAX_AUTO_POLLS = 24 // ~2 minutes at 5s intervals — bounded, not indefinite.
const POLL_INTERVAL_MS = 5000

export function WagerPaymentApproval({ wager, runId }: { wager: Wager; runId: string }) {
  const [intent, setIntent] = useState<SolanaPayIntent>()
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string>()
  const pollCount = useRef(0)

  async function requestPayment() {
    setLoading(true)
    setError(undefined)
    try {
      const created = await createSolanaPayIntentNative(runId, {
        amountSol: wager.stakeSol,
        label: `TxODDS wager ${wager.selection} (${wager.wagerId})`,
        memo: `wager:${wager.wagerId}`,
      })
      setIntent(created)
      pollCount.current = 0
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setLoading(false)
    }
  }

  async function checkStatus(reference: string) {
    try {
      const updated = await verifySolanaPayIntentNative(reference)
      setIntent(updated)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    }
  }

  // Bounded auto-poll while pending — stops on confirm/fail/expire or after
  // MAX_AUTO_POLLS, never runs forever.
  useEffect(() => {
    if (!intent || intent.status !== 'pending') return
    if (pollCount.current >= MAX_AUTO_POLLS) return
    const timer = setTimeout(() => {
      pollCount.current += 1
      void checkStatus(intent.reference)
    }, POLL_INTERVAL_MS)
    return () => clearTimeout(timer)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [intent])

  if (!intent) {
    return (
      <div className="wagerPaymentApproval">
        <button onClick={() => void requestPayment()} disabled={loading}>
          {loading ? 'Requesting…' : 'Request payment (wallet approval)'}
        </button>
        {error && <p className="muted" style={{ color: '#f87171', fontSize: '0.75rem' }}>{error}</p>}
      </div>
    )
  }

  return (
    <div className="wagerPaymentApproval">
      <div className="wagerPaymentStatusRow">
        <span className={`pill ${intent.status === 'confirmed' ? 'pillSuccess' : intent.status === 'pending' ? 'pillWarning' : 'pillDanger'}`}>
          {intent.status}
        </span>
        <span className="muted" style={{ fontSize: '0.72rem' }}>{intent.amountSol} SOL</span>
      </div>
      {intent.status === 'pending' && (
        <>
          <a
            className="wagerPaymentLink"
            href={intent.paymentUrl}
            target="_blank"
            rel="noreferrer"
          >
            Open in wallet to approve
          </a>
          <button className="secondary" onClick={() => void checkStatus(intent.reference)}>
            Check payment status
          </button>
        </>
      )}
      {error && <p className="muted" style={{ color: '#f87171', fontSize: '0.75rem' }}>{error}</p>}
    </div>
  )
}
