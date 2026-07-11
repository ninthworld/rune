import { useState } from 'react';
import { useGameStore } from '../store';

export function ConnectionScreen() {
  const [serverUrl, setServerUrl] = useState('');
  const status = useGameStore((state) => state.status);
  const connect = useGameStore((state) => state.connect);
  const disconnect = useGameStore((state) => state.disconnect);

  const handleConnect = (e: React.FormEvent) => {
    e.preventDefault();
    if (serverUrl.trim()) {
      connect(serverUrl.trim());
    }
  };

  const handleDisconnect = () => {
    disconnect();
    setServerUrl('');
  };

  const getStatusText = () => {
    switch (status) {
      case 'idle':
        return 'Waiting for connection';
      case 'connecting':
        return 'Connecting...';
      case 'open':
        return 'Connected';
      case 'closed':
        return 'Disconnected';
      default:
        return 'Unknown status';
    }
  };

  const getStatusColor = () => {
    switch (status) {
      case 'idle':
        return 'text-gray-500';
      case 'connecting':
        return 'text-yellow-500';
      case 'open':
        return 'text-green-500';
      case 'closed':
        return 'text-red-500';
      default:
        return 'text-gray-500';
    }
  };

  return (
    <div className="min-h-screen flex flex-col items-center justify-center bg-gray-900 text-white p-4">
      <div className="max-w-md w-full bg-gray-800 rounded-lg shadow-xl p-6 space-y-6">
        <h1 className="text-3xl font-bold text-center">RUNE Game Client</h1>

        <form onSubmit={handleConnect} className="space-y-4">
          <div>
            <label htmlFor="serverUrl" className="block text-sm font-medium mb-1">
              Server Address
            </label>
            <input
              id="serverUrl"
              type="text"
              value={serverUrl}
              onChange={(e) => setServerUrl(e.target.value)}
              placeholder="wss://example.com:8080"
              className="w-full px-3 py-2 bg-gray-700 border border-gray-600 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500"
              disabled={status === 'connecting' || status === 'open'}
            />
          </div>

          <div className="flex space-x-3">
            <button
              type="submit"
              disabled={status === 'connecting' || status === 'open' || !serverUrl.trim()}
              className={`flex-1 py-2 px-4 rounded-md font-medium ${
                status === 'connecting' || status === 'open' || !serverUrl.trim()
                  ? 'bg-gray-600 cursor-not-allowed'
                  : 'bg-blue-600 hover:bg-blue-700'
              }`}
            >
              {status === 'connecting' ? 'Connecting...' : 'Connect'}
            </button>

            {status === 'open' && (
              <button
                type="button"
                onClick={handleDisconnect}
                className="flex-1 py-2 px-4 bg-red-600 hover:bg-red-700 rounded-md font-medium"
              >
                Disconnect
              </button>
            )}
          </div>
        </form>

        <div className="pt-4">
          <div className="flex items-center justify-between">
            <span className="text-sm font-medium">Connection Status:</span>
            <span className={`text-sm font-bold ${getStatusColor()}`}>{getStatusText()}</span>
          </div>

          {status !== 'idle' && (
            <div className="mt-2 text-sm text-gray-300">
              {status === 'connecting' && <p>Attempting to connect to the server...</p>}
              {status === 'open' && <p>Connected successfully. Waiting for game state...</p>}
              {status === 'closed' && <p>Connection closed. Please reconnect.</p>}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
