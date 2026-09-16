import { useState } from "react";
import { MessageSquareText, X } from "lucide-react";
import { PopupPanel } from "./Popup";
import type { UserMessageNavigation } from "./useTaskChatScroll";

export function UserMessagePicker({ navigation }: { navigation: UserMessageNavigation }) {
  const [open, setOpen] = useState(false);
  return (
    <PopupPanel
      className="user-message-picker"
      label="Jump to a message"
      onOpenChange={setOpen}
      open={open}
      placement="bottom-end"
      trigger={(props) => (
        <button {...props} aria-label="Jump to a message" className="user-message-picker-trigger" title="Jump to a message" type="button">
          <MessageSquareText aria-hidden="true" size={18} />
        </button>
      )}
    >
      <div className="user-message-picker-heading">
        <strong>{navigation.hasEarlier ? "Your messages · loaded history" : "Your messages"}</strong>
        <button aria-label="Close message navigation" onClick={() => setOpen(false)} type="button">
          <X aria-hidden="true" size={18} />
        </button>
      </div>
      <div className="user-message-picker-list">
        {navigation.hasEarlier ? (
          <button disabled={navigation.pendingPrevious} onClick={() => {
            if (navigation.currentIndex > 0) navigation.goFirst();
            else navigation.goPrevious();
            setOpen(false);
          }} type="button">
            {navigation.pendingPrevious ? "Loading earlier messages…" : navigation.currentIndex > 0 ? "Go to start of loaded history" : "Go to earlier messages"}
          </button>
        ) : null}
        {navigation.anchors.map((anchor, index) => {
          const text = anchor.text.trim() || "Attachment-only message";
          return (
            <button
              aria-current={index === navigation.currentIndex ? "true" : undefined}
              key={anchor.key}
              ref={index === navigation.currentIndex ? (element) => element?.scrollIntoView({ block: "nearest" }) : undefined}
              onClick={() => {
                navigation.navigateTo(anchor);
                setOpen(false);
              }}
              title={text}
              type="button"
            >
              <span className="user-message-picker-number" aria-hidden="true">{index + 1}</span>
              <span className="user-message-picker-text">{text}</span>
            </button>
          );
        })}
      </div>
    </PopupPanel>
  );
}
