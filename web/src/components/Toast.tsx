import { useEffect } from "react";

export type ToastMsg = { id: string; title: string; message?: string };

export function ToastHost(props: { items: ToastMsg[]; onRemove: (id: string) => void }) {
  return (
    <div className="toastWrap">
      {props.items.map(t => <Toast key={t.id} item={t} onRemove={props.onRemove} />)}
    </div>
  );
}

function Toast(props: { item: ToastMsg; onRemove: (id: string) => void }) {
  useEffect(() => {
    const h = setTimeout(() => props.onRemove(props.item.id), 4500);
    return () => clearTimeout(h);
  }, [props]);

  return (
    <div className="toast">
      <div className="t">{props.item.title}</div>
      {props.item.message ? <div className="m">{props.item.message}</div> : null}
    </div>
  );
}
