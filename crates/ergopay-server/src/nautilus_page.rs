//! Nautilus signing page HTML
//!
//! Self-contained HTML/JS page that connects to Nautilus wallet,
//! signs and submits the transaction.

/// Generate the Nautilus signing page HTML
///
/// The page will:
/// 1. Check if Nautilus is installed
/// 2. Connect to Nautilus (user approves)
/// 3. Fetch unsigned tx from /nautilus/tx/{id}
/// 4. Sign the transaction (user approves)
/// 5. Submit the transaction
/// 6. POST txId to /callback/{id}
/// 7. Show success/error status
pub fn generate_signing_page(request_id: &str, message: &str, host: &str, port: u16) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Sign Transaction - Citadel</title>
    <style>
        * {{ margin: 0; padding: 0; box-sizing: border-box; }}
        body {{
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            background: linear-gradient(135deg, #1a1a2e 0%, #16213e 100%);
            min-height: 100vh;
            display: flex;
            align-items: center;
            justify-content: center;
            color: #fff;
        }}
        .container {{
            background: rgba(255,255,255,0.05);
            border-radius: 16px;
            padding: 40px;
            max-width: 480px;
            width: 90%;
            text-align: center;
            border: 1px solid rgba(255,255,255,0.1);
        }}
        h1 {{ font-size: 24px; margin-bottom: 8px; }}
        .message {{ color: rgba(255,255,255,0.7); margin-bottom: 32px; font-size: 14px; }}
        .status {{
            padding: 20px;
            border-radius: 8px;
            margin-bottom: 24px;
            font-size: 16px;
            display: flex;
            align-items: center;
            justify-content: center;
            gap: 8px;
        }}
        .status.loading {{ background: rgba(59, 130, 246, 0.2); border: 1px solid rgba(59, 130, 246, 0.3); }}
        .status.success {{ background: rgba(34, 197, 94, 0.2); border: 1px solid rgba(34, 197, 94, 0.3); }}
        .status.error {{ background: rgba(239, 68, 68, 0.2); border: 1px solid rgba(239, 68, 68, 0.3); }}
        .spinner {{
            width: 20px; height: 20px;
            border: 2px solid rgba(255,255,255,0.3);
            border-top-color: #fff;
            border-radius: 50%;
            animation: spin 1s linear infinite;
        }}
        @keyframes spin {{ to {{ transform: rotate(360deg); }} }}
        .tx-id {{
            font-family: monospace; font-size: 12px;
            background: rgba(0,0,0,0.3);
            padding: 8px 12px; border-radius: 4px;
            margin-top: 12px; word-break: break-all;
        }}
        button {{
            background: #3b82f6; color: white; border: none;
            padding: 12px 24px; border-radius: 8px;
            font-size: 16px; cursor: pointer; transition: background 0.2s;
        }}
        button:hover {{ background: #2563eb; }}
        .hidden {{ display: none; }}
        .install-link {{ color: #3b82f6; text-decoration: none; }}
        .install-link:hover {{ text-decoration: underline; }}
    </style>
</head>
<body>
    <div class="container">
        <h1>Sign Transaction</h1>
        <p class="message" id="message"></p>
        <div id="status" class="status loading">
            <div class="spinner" id="spinner"></div>
            <span id="status-text">Checking for Nautilus wallet...</span>
        </div>
        <div id="tx-result" class="hidden">
            <div class="tx-id" id="tx-id"></div>
        </div>
        <div id="actions" class="hidden">
            <button onclick="window.close()">Close Window</button>
        </div>
        <div id="install-prompt" class="hidden">
            <p>Nautilus wallet extension is required.</p>
            <br>
            <a href="https://chrome.google.com/webstore/detail/nautilus-wallet/gjlmehlldlphhljhpnlddaodbjjcchai"
               target="_blank" class="install-link">Install Nautilus for Chrome</a>
        </div>
    </div>
    <script>
        const REQUEST_ID = "{request_id}";
        const BASE_URL = "http://{host}:{port}";
        const MESSAGE = "{message}";

        const statusEl = document.getElementById('status');
        const statusText = document.getElementById('status-text');
        const spinner = document.getElementById('spinner');
        const txResult = document.getElementById('tx-result');
        const txIdEl = document.getElementById('tx-id');
        const actions = document.getElementById('actions');
        const installPrompt = document.getElementById('install-prompt');
        const messageEl = document.getElementById('message');

        messageEl.textContent = MESSAGE;

        function setStatus(text, type, showSpinner) {{
            statusText.textContent = text;
            statusEl.className = 'status ' + type;
            spinner.style.display = showSpinner ? 'block' : 'none';
        }}

        function showError(text) {{ setStatus(text, 'error', false); }}

        function showSuccess(text, txId) {{
            setStatus(text, 'success', false);
            if (txId) {{
                txIdEl.textContent = txId;
                txResult.classList.remove('hidden');
            }}
            actions.classList.remove('hidden');
            // Auto-close after 2 seconds (works in --app mode windows)
            setTimeout(() => {{
                try {{ window.close(); }} catch(e) {{}}
            }}, 2000);
        }}

        async function signTransaction() {{
            try {{
                await new Promise(r => setTimeout(r, 500));

                if (!window.ergoConnector || !window.ergoConnector.nautilus) {{
                    showError('Nautilus wallet not detected');
                    installPrompt.classList.remove('hidden');
                    return;
                }}

                setStatus('Please approve connection in Nautilus...', 'loading', true);
                const connected = await window.ergoConnector.nautilus.connect();
                if (!connected) {{ showError('Connection rejected by user'); return; }}

                setStatus('Fetching transaction...', 'loading', true);
                const txResponse = await fetch(BASE_URL + '/nautilus/tx/' + REQUEST_ID);
                if (!txResponse.ok) {{
                    showError('Failed to fetch transaction: ' + await txResponse.text());
                    return;
                }}
                const unsignedTx = await txResponse.json();

                setStatus('Please approve transaction in Nautilus...', 'loading', true);
                const signedTx = await ergo.sign_tx(unsignedTx);

                setStatus('Submitting transaction...', 'loading', true);
                const txId = await ergo.submit_tx(signedTx);

                await fetch(BASE_URL + '/callback/' + REQUEST_ID, {{
                    method: 'POST',
                    headers: {{ 'Content-Type': 'application/json' }},
                    body: JSON.stringify({{ txId: txId }})
                }});

                showSuccess('Transaction submitted successfully!', txId);
            }} catch (error) {{
                console.error('Signing error:', error);
                const info = error.info || error.message || 'Unknown error';
                if (info.toLowerCase().includes('rejected') || info.toLowerCase().includes('denied') || info.toLowerCase().includes('cancelled')) {{
                    showError('Transaction rejected by user');
                }} else if (error.code !== undefined) {{
                    showError('Transaction failed: ' + info);
                }} else {{
                    showError('Error: ' + info);
                }}
            }}
        }}

        signTransaction();
    </script>
</body>
</html>"#,
        message = escape_js_string(message),
        request_id = request_id,
        host = host,
        port = port
    )
}

/// Generate the Nautilus wallet connect page HTML
///
/// The page will:
/// 1. Check if Nautilus is installed
/// 2. Connect to Nautilus (user approves)
/// 3. Get wallet address
/// 4. POST address to /connect/{id}
/// 5. Show success and close
pub fn generate_connect_page(request_id: &str, host: &str, port: u16) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Connect Wallet - Citadel</title>
    <style>
        * {{ margin: 0; padding: 0; box-sizing: border-box; }}
        body {{
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            background: linear-gradient(135deg, #1a1a2e 0%, #16213e 100%);
            min-height: 100vh;
            display: flex;
            align-items: center;
            justify-content: center;
            color: #fff;
        }}
        .container {{
            background: rgba(255,255,255,0.05);
            border-radius: 16px;
            padding: 40px;
            max-width: 480px;
            width: 90%;
            text-align: center;
            border: 1px solid rgba(255,255,255,0.1);
        }}
        h1 {{ font-size: 24px; margin-bottom: 8px; }}
        .subtitle {{ color: rgba(255,255,255,0.7); margin-bottom: 32px; font-size: 14px; }}
        .status {{
            padding: 20px;
            border-radius: 8px;
            margin-bottom: 24px;
            font-size: 16px;
            display: flex;
            align-items: center;
            justify-content: center;
            gap: 8px;
        }}
        .status.loading {{ background: rgba(59, 130, 246, 0.2); border: 1px solid rgba(59, 130, 246, 0.3); }}
        .status.success {{ background: rgba(34, 197, 94, 0.2); border: 1px solid rgba(34, 197, 94, 0.3); }}
        .status.error {{ background: rgba(239, 68, 68, 0.2); border: 1px solid rgba(239, 68, 68, 0.3); }}
        .spinner {{
            width: 20px; height: 20px;
            border: 2px solid rgba(255,255,255,0.3);
            border-top-color: #fff;
            border-radius: 50%;
            animation: spin 1s linear infinite;
        }}
        @keyframes spin {{ to {{ transform: rotate(360deg); }} }}
        .address {{
            font-family: monospace; font-size: 11px;
            background: rgba(0,0,0,0.3);
            padding: 8px 12px; border-radius: 4px;
            margin-top: 12px; word-break: break-all;
        }}
        button {{
            background: #3b82f6; color: white; border: none;
            padding: 12px 24px; border-radius: 8px;
            font-size: 16px; cursor: pointer; transition: background 0.2s;
        }}
        button:hover {{ background: #2563eb; }}
        .hidden {{ display: none; }}
        .install-link {{ color: #3b82f6; text-decoration: none; }}
        .install-link:hover {{ text-decoration: underline; }}
    </style>
</head>
<body>
    <div class="container">
        <h1>Connect Wallet</h1>
        <p class="subtitle">Connect your Nautilus wallet to Citadel</p>
        <div id="status" class="status loading">
            <div class="spinner" id="spinner"></div>
            <span id="status-text">Checking for Nautilus wallet...</span>
        </div>
        <div id="address-result" class="hidden">
            <div class="address" id="address"></div>
        </div>
        <div id="actions" class="hidden">
            <button onclick="window.close()">Close Window</button>
        </div>
        <div id="install-prompt" class="hidden">
            <p>Nautilus wallet extension is required.</p>
            <br>
            <a href="https://chrome.google.com/webstore/detail/nautilus-wallet/gjlmehlldlphhljhpnlddaodbjjcchai"
               target="_blank" class="install-link">Install Nautilus for Chrome</a>
        </div>
    </div>
    <script>
        const REQUEST_ID = "{request_id}";
        const BASE_URL = "http://{host}:{port}";

        const statusEl = document.getElementById('status');
        const statusText = document.getElementById('status-text');
        const spinner = document.getElementById('spinner');
        const addressResult = document.getElementById('address-result');
        const addressEl = document.getElementById('address');
        const actions = document.getElementById('actions');
        const installPrompt = document.getElementById('install-prompt');

        function setStatus(text, type, showSpinner) {{
            statusText.textContent = text;
            statusEl.className = 'status ' + type;
            spinner.style.display = showSpinner ? 'block' : 'none';
        }}

        function showError(text) {{ setStatus(text, 'error', false); }}

        function showSuccess(text, address) {{
            setStatus(text, 'success', false);
            if (address) {{
                addressEl.textContent = address;
                addressResult.classList.remove('hidden');
            }}
            actions.classList.remove('hidden');
        }}

        async function connectWallet() {{
            try {{
                await new Promise(r => setTimeout(r, 500));

                if (!window.ergoConnector || !window.ergoConnector.nautilus) {{
                    showError('Nautilus wallet not detected');
                    installPrompt.classList.remove('hidden');
                    return;
                }}

                setStatus('Please approve connection in Nautilus...', 'loading', true);
                const connected = await window.ergoConnector.nautilus.connect();
                if (!connected) {{ showError('Connection rejected by user'); return; }}

                setStatus('Getting wallet address...', 'loading', true);
                const addresses = await ergo.get_used_addresses();
                if (!addresses || addresses.length === 0) {{
                    showError('No addresses found in wallet');
                    return;
                }}
                const address = addresses[0];

                setStatus('Connecting to app...', 'loading', true);
                const response = await fetch(BASE_URL + '/connect/' + REQUEST_ID + '?address=' + encodeURIComponent(address));
                if (!response.ok) {{
                    showError('Failed to connect: ' + await response.text());
                    return;
                }}

                showSuccess('Wallet connected successfully!', address);

                // Auto-close after 2 seconds (works in --app mode windows)
                setTimeout(() => {{
                    try {{ window.close(); }} catch(e) {{}}
                }}, 2000);
            }} catch (error) {{
                console.error('Connection error:', error);
                showError('Error: ' + (error.message || error.info || 'Unknown error'));
            }}
        }}

        connectWallet();
    </script>
</body>
</html>"#,
        request_id = request_id,
        host = host,
        port = port
    )
}

/// Escape string for safe use in JavaScript
fn escape_js_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\'', "\\'")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
}
