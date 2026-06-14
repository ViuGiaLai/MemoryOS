import React from "react";

interface ScanButtonProps {
  status: "idle" | "scanning" | "completed" | "error" | "stopped";
  folderCount: number;
  onStart: () => void;
  onStop: () => void;
}

export const ScanButton: React.FC<ScanButtonProps> = ({
  status,
  folderCount,
  onStart,
  onStop,
}) => {
  const isScanning = status === "scanning";
  const canStart = !isScanning && folderCount > 0;

  if (isScanning) {
    return (
      <button className="scan-btn scan-btn-stop" onClick={onStop} title="Dừng quét">
        <span className="scan-btn-spinner">⏳</span>
        <span>Đang quét...</span>
      </button>
    );
  }

  return (
    <button
      className="scan-btn scan-btn-start"
      onClick={onStart}
      disabled={!canStart}
      title={
        folderCount === 0
          ? "Chưa có thư mục nào. Vui lòng thêm thư mục trước."
          : "Bắt đầu quét và index files"
      }
    >
      <span className="scan-btn-icon">
        {status === "completed" ? "✅" : status === "error" ? "❌" : "🔍"}
      </span>
      <span>
        {status === "completed"
          ? "Quét lại"
          : status === "error"
          ? "Thử lại"
          : "Bắt đầu quét"}
      </span>
    </button>
  );
};
