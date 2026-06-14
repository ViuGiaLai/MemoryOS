import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";

/**
 * Hook for managing scanned folders via Tauri IPC commands.
 * Provides state and actions for the FolderPicker components.
 */
export function useFolders() {
  const [folders, setFolders] = useState<string[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [isAdding, setIsAdding] = useState(false);

  // Load folders on mount
  useEffect(() => {
    loadFolders();
  }, []);

  const loadFolders = useCallback(async () => {
    try {
      setIsLoading(true);
      const result = await invoke<string[]>("get_folders");
      setFolders(result);
    } catch (e) {
      console.error("Failed to load folders:", e);
    } finally {
      setIsLoading(false);
    }
  }, []);

  const addFolder = useCallback(async () => {
    try {
      setIsAdding(true);
      const result = await invoke<string[]>("add_folder");
      setFolders(result);
    } catch (e) {
      console.error("Failed to add folder:", e);
    } finally {
      setIsAdding(false);
    }
  }, []);

  const removeFolder = useCallback(async (folder: string) => {
    try {
      const result = await invoke<string[]>("remove_folder", { folder });
      setFolders(result);
    } catch (e) {
      console.error("Failed to remove folder:", e);
    }
  }, []);

  return {
    folders,
    isLoading,
    isAdding,
    addFolder,
    removeFolder,
    reload: loadFolders,
  };
}
