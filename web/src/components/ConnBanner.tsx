export function ConnBanner({ connStatus, lastUpdated }: { connStatus: string; lastUpdated: Date | null }) {
  if (connStatus === "offline") {
    return (
      <div className="conn-banner offline">
        Backend unreachable. Last data: {lastUpdated ? lastUpdated.toLocaleTimeString() : "never"}. Retrying automatically...
      </div>
    );
  }
  if (connStatus === "loading") {
    return <div className="conn-banner loading">Connecting to backend...</div>;
  }
  return null;
}
