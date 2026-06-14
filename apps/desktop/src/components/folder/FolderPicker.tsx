import React from "react";

interface FolderPickerProps {
  onAddFolder: () => void;
  isAdding: boolean;
}

/**
 * Button component that opens the native folder selection dialog
 * via the `add_folder` Tauri command.
 */
export function FolderPicker({ onAddFolder, isAdding }: FolderPickerProps) {
  return (
    <button
      className="folder-picker-btn"
      onClick={onAddFolder}
      disabled={isAdding}
      title="Chọn thư mục để index"
    >
      {isAdding ? (
        <span className="folder-picker-loading">⏳</span>
      ) : (
        <>
          <span className="folder-picker-icon">📂</span>
          <span className="folder-picker-text">Chọn thư mục</span>
        </>
      )}
    </button>
  );
}
