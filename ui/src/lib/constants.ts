/** Supported document file extensions for the file picker dialog. */
export const SUPPORTED_EXTENSIONS = [
  "txt",
  "pdf",
  "xlsx",
  "xls",
  "png",
  "jpg",
  "jpeg",
  "tiff",
  "tif",
];

/** File filter configuration for Tauri's open dialog. */
export const FILE_FILTER = {
  name: "Documents",
  extensions: SUPPORTED_EXTENSIONS,
};
