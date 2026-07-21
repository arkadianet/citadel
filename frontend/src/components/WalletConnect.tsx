import { useState, useEffect } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { QRCodeSVG } from 'qrcode.react'
import { Button, Spinner } from './ui'
import './WalletConnect.css'

interface WalletConnectResponse {
  request_id: string
  qr_url: string
  nautilus_url: string
}

interface ConnectionStatusResponse {
  status: string
  address: string | null
  addresses?: string[]
}

interface WalletConnectProps {
  onConnected: (address: string, addresses?: string[]) => void
  onCancel?: () => void
  onClose?: () => void
}

type ConnectMethod = 'choose' | 'mobile' | 'nautilus'

export function WalletConnect({ onConnected, onCancel, onClose }: WalletConnectProps) {
  const [requestId, setRequestId] = useState<string | null>(null)
  const [qrUrl, setQrUrl] = useState<string | null>(null)
  const [nautilusUrl, setNautilusUrl] = useState<string | null>(null)
  const [status, setStatus] = useState<'starting' | 'waiting' | 'connected' | 'error'>('starting')
  const [error, setError] = useState<string | null>(null)
  const [connectMethod, setConnectMethod] = useState<ConnectMethod>('choose')

  // Start connection request
  useEffect(() => {
    let cancelled = false

    const startConnect = async () => {
      try {
        const response = await invoke<WalletConnectResponse>('start_wallet_connect')
        if (!cancelled) {
          setRequestId(response.request_id)
          setQrUrl(response.qr_url)
          setNautilusUrl(response.nautilus_url)
          setStatus('waiting')
        }
      } catch (e) {
        if (!cancelled) {
          setError(String(e))
          setStatus('error')
        }
      }
    }

    startConnect()

    return () => {
      cancelled = true
    }
  }, [])

  // Poll for connection status
  useEffect(() => {
    if (!requestId || status !== 'waiting') return

    const pollInterval = setInterval(async () => {
      try {
        const response = await invoke<ConnectionStatusResponse>('get_connection_status', {
          requestId,
        })

        if (response.status === 'connected' && response.address) {
          setStatus('connected')
          onConnected(response.address, response.addresses)
        } else if (response.status === 'expired') {
          setError('Connection request expired. Please try again.')
          setStatus('error')
        } else if (response.status.startsWith('failed')) {
          setError(response.status)
          setStatus('error')
        }
      } catch (e) {
        console.error('Failed to poll connection status:', e)
      }
    }, 1000)

    return () => clearInterval(pollInterval)
  }, [requestId, status, onConnected])

  // Close on Escape key
  useEffect(() => {
    const handleEscape = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && onClose) {
        onClose()
      }
    }
    window.addEventListener('keydown', handleEscape)
    return () => window.removeEventListener('keydown', handleEscape)
  }, [onClose])

  const handleNautilusConnect = async () => {
    if (!nautilusUrl) return
    setConnectMethod('nautilus')
    try {
      await invoke('open_nautilus', { nautilusUrl })
    } catch (e) {
      setError(String(e))
      setStatus('error')
    }
  }

  const handleMobileConnect = () => {
    setConnectMethod('mobile')
  }

  const handleBackToChoice = () => {
    setConnectMethod('choose')
  }

  if (status === 'starting') {
    return (
      <div className="wallet-connect">
        <div className="wallet-connect-loading">
          <Spinner size={32} />
          <p>Starting connection...</p>
        </div>
      </div>
    )
  }

  if (status === 'error') {
    return (
      <div className="wallet-connect">
        <div className="wallet-connect-error">
          <p className="error-message">{error}</p>
          <Button variant="primary" onClick={() => window.location.reload()}>
            Try Again
          </Button>
        </div>
      </div>
    )
  }

  if (status === 'connected') {
    return (
      <div className="wallet-connect">
        <div className="wallet-connect-success">
          <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="var(--ds-success)" strokeWidth="2">
            <path d="M22 11.08V12a10 10 0 1 1-5.93-9.14" />
            <polyline points="22 4 12 14.01 9 11.01" />
          </svg>
          <p>Wallet connected!</p>
        </div>
      </div>
    )
  }

  // Show wallet selection
  if (connectMethod === 'choose') {
    return (
      <div className="wallet-connect">
        <div className="wallet-connect-content">
          <h3>Connect Wallet</h3>
          <p className="wallet-connect-subtitle">
            Choose your wallet connection method
          </p>

          <div className="wallet-options">
            <button className="wallet-option" onClick={handleNautilusConnect}>
              <div className="wallet-option-icon">
                <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <rect x="2" y="3" width="20" height="14" rx="2" />
                  <path d="M8 21h8" />
                  <path d="M12 17v4" />
                </svg>
              </div>
              <div className="wallet-option-info">
                <span className="wallet-option-name">Nautilus Extension</span>
                <span className="wallet-option-desc">Browser extension wallet</span>
              </div>
            </button>

            <button className="wallet-option" onClick={handleMobileConnect}>
              <div className="wallet-option-icon">
                <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <rect x="5" y="2" width="14" height="20" rx="2" />
                  <line x1="12" y1="18" x2="12.01" y2="18" />
                </svg>
              </div>
              <div className="wallet-option-info">
                <span className="wallet-option-name">Mobile Wallet</span>
                <span className="wallet-option-desc">Scan QR code with Ergo Wallet</span>
              </div>
            </button>
          </div>

          {onCancel && (
            <Button variant="secondary" onClick={onCancel}>
              Cancel
            </Button>
          )}
        </div>
      </div>
    )
  }

  // Show Nautilus waiting state
  if (connectMethod === 'nautilus') {
    return (
      <div className="wallet-connect">
        <div className="wallet-connect-content">
          <h3>Connect with Nautilus</h3>
          <p className="wallet-connect-subtitle">
            Approve the Nautilus popup in your browser to connect
          </p>

          <div className="nautilus-waiting">
            <div className="nautilus-icon">
              <svg width="64" height="64" viewBox="0 0 24 24" fill="none" stroke="var(--emerald-400)" strokeWidth="1.5">
                <rect x="2" y="3" width="20" height="14" rx="2" />
                <path d="M8 21h8" />
                <path d="M12 17v4" />
              </svg>
            </div>
            <p className="wallet-connect-status">
              <span className="status-dot" />
              Waiting for wallet connection...
            </p>
          </div>

          <div className="wallet-connect-actions">
            <Button variant="secondary" onClick={handleBackToChoice}>
              Back
            </Button>
            <Button variant="primary" onClick={handleNautilusConnect}>
              Open Nautilus Again
            </Button>
          </div>
        </div>
      </div>
    )
  }

  // Show QR code for mobile wallet
  return (
    <div className="wallet-connect">
      <div className="wallet-connect-content">
        <h3>Connect with Mobile</h3>
        <p className="wallet-connect-subtitle">
          Scan with your Ergo mobile wallet
        </p>

        <div className="qr-container">
          {qrUrl && (
            <QRCodeSVG
              value={qrUrl}
              size={200}
              level="M"
              includeMargin
              bgColor="white"
              fgColor="black"
            />
          )}
        </div>

        <p className="wallet-connect-status">
          <span className="status-dot" />
          Waiting for connection...
        </p>

        <div className="wallet-connect-actions">
          <Button variant="secondary" onClick={handleBackToChoice}>
            Back
          </Button>
          {onCancel && (
            <Button variant="secondary" onClick={onCancel}>
              Cancel
            </Button>
          )}
        </div>
      </div>
    </div>
  )
}
