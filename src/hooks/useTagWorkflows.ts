import { useState, type Dispatch, type SetStateAction } from "react";
import { api, type Tag } from "../lib/tauri";

/** The "tag selection" modal: create-and-assign or apply an existing tag. */
export function useTagWorkflows({
  selectedIds,
  refreshTags,
  loadAssets,
  setError,
}: {
  selectedIds: string[];
  refreshTags: () => Promise<void>;
  loadAssets: () => Promise<void>;
  setError: Dispatch<SetStateAction<string | null>>;
}) {
  const [tagModal, setTagModal] = useState(false);
  const [tagName, setTagName] = useState("");

  async function submitTagModal() {
    if (!selectedIds.length) return;
    const name = tagName.trim();
    if (!name) {
      setError("Enter a tag name");
      return;
    }
    try {
      const tag = await api.createTagAndAssign(name, selectedIds);
      setTagModal(false);
      setTagName("");
      setError(`Tagged ${selectedIds.length} item(s) with “${tag.name}”`);
      await refreshTags();
      await loadAssets();
    } catch (e) {
      setError(String(e));
    }
  }

  async function applyExistingTag(tag: Tag) {
    if (!selectedIds.length) return;
    try {
      const count = await api.tagAssets(tag.id, selectedIds);
      setTagModal(false);
      setTagName("");
      setError(
        count > 0
          ? `Tagged ${count} item(s) with “${tag.name}”`
          : `All selected items already have the “${tag.name}” tag`,
      );
      await refreshTags();
      await loadAssets();
    } catch (e) {
      setError(String(e));
    }
  }

  return {
    tagModal,
    setTagModal,
    tagName,
    setTagName,
    submitTagModal,
    applyExistingTag,
  };
}
