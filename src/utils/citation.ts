export interface DocumentCitationOptions {
  fileName: string;
  titlePath?: string | null;
  heading?: string | null;
  locationParts?: string[];
}

export interface DirectoryCitationOptions {
  displayName?: string | null;
  path: string;
  docs: number;
  chunks: number;
}

export const resolveDocumentTitlePath = (options: Pick<DocumentCitationOptions, "titlePath" | "heading"> & { fileName?: string | null }) => {
  return (
    options.titlePath?.trim() ||
    options.heading?.trim() ||
    options.fileName?.trim() ||
    ""
  );
};

export interface DocumentLocationPartsOptions {
  page?: number | null;
  paragraph?: number | null;
  pageLabel?: string;
  paragraphLabel?: string;
}

export const buildDocumentLocationParts = (options: DocumentLocationPartsOptions) => {
  const parts: string[] = [];

  if (typeof options.page === "number" && Number.isFinite(options.page)) {
    parts.push(options.pageLabel?.trim() || `Page ${options.page}`);
  }

  if (typeof options.paragraph === "number" && Number.isFinite(options.paragraph)) {
    parts.push(options.paragraphLabel?.trim() || `Para ${options.paragraph}`);
  }

  return parts;
};

export const formatDocumentCitation = (options: DocumentCitationOptions) => {
  const titlePath = resolveDocumentTitlePath(options);
  const locationParts = options.locationParts?.filter((part) => part.trim().length > 0) ?? [];
  const parts = [`《${options.fileName}》`, titlePath || options.fileName, ...locationParts];
  return parts.join(" / ");
};

export const formatDirectoryCitation = (options: DirectoryCitationOptions) => {
  const title = options.displayName?.trim() || options.path.trim() || "";
  return `《${title || options.path}》 / ${options.path} / ${options.docs} 文档 / ${options.chunks} 切片`;
};
