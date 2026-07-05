import { useEffect, useState } from 'react'
import { QRCodeSVG } from 'qrcode.react'
import type { AgentRun, SolanaPayIntent } from '../types'
import { native, verifySolanaPayIntentNative } from '../desktop/transport'

// SettlementLab presents the current run's settlement receipt without owning
// settlement authority. Rust creates and verifies Solana Pay intents; React only
// renders the QR-safe payload and asks for a backend re-check.
export function SettlementLab({ run }: { run?: AgentRun }) {
  const settlement = run?.settlement
  const paymentUrl = settlement?.paymentUrl
  const paymentReference = settlement?.paymentReference ?? settlement?.reference
  const [checkedIntent, setCheckedIntent] = useState<SolanaPayIntent>()
  const [verifyMessage, setVerifyMessage] = useState('')
  const [checking, setChecking] = useState(false)

  useEffect(() => {
    setCheckedIntent(undefined)
    setVerifyMessage('')
  }, [run?.runId, paymentReference])

  async function verifyPayment() {
    if (!native || !paymentReference) return
    setChecking(true)
    setVerifyMessage('')
    try {
      const intent = await verifySolanaPayIntentNative(paymentReference)
      setCheckedIntent(intent)
      setVerifyMessage(`Solana Pay reference ${intent.status}`)
    } catch (err) {
      setVerifyMessage(err instanceof Error ? err.message : String(err))
    } finally {
      setChecking(false)
    }
  }

  const paymentStatus = checkedIntent?.status ?? settlement?.paymentStatus ?? 'not_created'
  const signature = checkedIntent?.signature ?? settlement?.paymentSignature
  const amount = checkedIntent?.amountSol ?? settlement?.paymentAmountSol
  const recipient = checkedIntent?.recipient ?? settlement?.paymentRecipient

  return (
    <article className="card">
      <div className="cardHead">
        <h2>Settlement Lab</h2>
        <span className="pill">{settlement?.rail ?? 'solana_pay'}</span>
      </div>
      <p className="muted">Solana Pay is the primary devnet payment and proof rail. CoralOS can enrich the receipt when configured.</p>
      {paymentUrl ? (
        <div className="payGrid">
          <div className="qrBox">
            <QRCodeSVG value={paymentUrl} size={160} bgColor="#ffffff" fgColor="#111827" level="M" />
          </div>
          <div className="receipt">
            <span>Payment status</span><strong>{paymentStatus}</strong>
            <span>Amount</span><strong>{amount ? `${amount} SOL` : '-'}</strong>
            <span>Recipient</span><code>{recipient ?? '-'}</code>
            <span>Reference</span><code>{paymentReference ?? '-'}</code>
            <span>Memo</span><code>{settlement?.paymentMemo ?? '-'}</code>
            <span>Signature</span><code>{signature ?? '-'}</code>
          </div>
        </div>
      ) : null}
      <div className="receipt">
        <span>Status</span><strong>{settlement?.status ?? 'not_started'}</strong>
        <span>Reference</span><code>{settlement?.reference ?? '-'}</code>
        <span>Triton observed</span><strong>{settlement?.tritonObserved ? 'yes' : 'not yet'}</strong>
        <span>Explorer</span><code>{settlement?.explorerUrl ?? '-'}</code>
      </div>
      <button className="secondary" disabled={!paymentReference || checking || !native} onClick={verifyPayment}>
        {checking ? 'Checking payment' : 'Verify Solana Pay reference'}
      </button>
      {verifyMessage ? <p className="muted">{verifyMessage}</p> : null}
    </article>
  )
}
