import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/tauri';

interface HealthResponse {
  status: string;
  version: string;
}

interface NodeStatus {
  connected: boolean;
  url: string;
  network: string;
  chain_height: number;
  indexed_height: number | null;
  capability_tier: string;
  index_lag: number | null;
}

function App() {
  const [health, setHealth] = useState<HealthResponse | null>(null);
  const [nodeStatus, setNodeStatus] = useState<NodeStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    loadStatus();
  }, []);

  async function loadStatus() {
    setLoading(true);
    setError(null);

    try {
      // Get health status
      const healthResult = await invoke<HealthResponse>('health_check');
      setHealth(healthResult);

      // Get node status
      const nodeResult = await invoke<NodeStatus>('get_node_status');
      setNodeStatus(nodeResult);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }

  const getStatusColor = (tier: string) => {
    switch (tier) {
      case 'Full':
        return '#22c55e'; // green
      case 'IndexLagging':
        return '#f59e0b'; // amber
      default:
        return '#ef4444'; // red
    }
  };

  return (
    <div className="container">
      <h1>Citadel</h1>

      <div className="card">
        <h2>API Status</h2>
        {loading ? (
          <p>Loading...</p>
        ) : error ? (
          <p className="error">{error}</p>
        ) : health ? (
          <div className="status-row">
            <span className="status-indicator" style={{ backgroundColor: '#22c55e' }} />
            <span>
              {health.status} - v{health.version}
            </span>
          </div>
        ) : null}
      </div>

      <div className="card">
        <h2>Node Connection</h2>
        {loading ? (
          <p>Connecting...</p>
        ) : nodeStatus ? (
          <div className="node-status">
            <div className="status-row">
              <span
                className="status-indicator"
                style={{
                  backgroundColor: nodeStatus.connected ? '#22c55e' : '#ef4444',
                }}
              />
              <span>{nodeStatus.connected ? 'Connected' : 'Disconnected'}</span>
            </div>

            <div className="info-grid">
              <div className="info-item">
                <label>URL</label>
                <span>{nodeStatus.url}</span>
              </div>
              <div className="info-item">
                <label>Network</label>
                <span>{nodeStatus.network}</span>
              </div>
              <div className="info-item">
                <label>Chain Height</label>
                <span>{nodeStatus.chain_height.toLocaleString()}</span>
              </div>
              <div className="info-item">
                <label>Indexed Height</label>
                <span>
                  {nodeStatus.indexed_height?.toLocaleString() ?? 'N/A'}
                </span>
              </div>
              <div className="info-item">
                <label>Capability Tier</label>
                <span
                  style={{
                    color: getStatusColor(nodeStatus.capability_tier),
                    fontWeight: 'bold',
                  }}
                >
                  {nodeStatus.capability_tier}
                </span>
              </div>
              {nodeStatus.index_lag !== null && (
                <div className="info-item">
                  <label>Index Lag</label>
                  <span>{nodeStatus.index_lag} blocks</span>
                </div>
              )}
            </div>
          </div>
        ) : null}

        <button onClick={loadStatus} disabled={loading}>
          {loading ? 'Loading...' : 'Refresh'}
        </button>
      </div>

      <div className="card">
        <h2>SigmaUSD Protocol</h2>
        <p className="placeholder">
          Connect to an indexed Ergo node to view SigmaUSD state.
        </p>
      </div>
    </div>
  );
}

export default App;
