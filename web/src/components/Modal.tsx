import React from "react";

export function Modal(props: { title: string; onClose: () => void; children: React.ReactNode }) {
  return (
    <div className="modalOverlay" role="dialog" aria-modal="true" onMouseDown={props.onClose}>
      <div className="modal" onMouseDown={(e) => e.stopPropagation()}>
        <div className="modalH">
          <h3>{props.title}</h3>
          <button className="btn" onClick={props.onClose}>Close</button>
        </div>
        <div className="modalB">{props.children}</div>
      </div>
    </div>
  );
}
