use crate::docmind::models::{CollectionItemView, CollectionView, TagView};
use crate::docmind::storage::db::Database;
use crate::docmind::storage::types::{
    CollectionItemInput, CollectionItemPatchInput, CollectionPatchInput, TagPatchInput,
};

#[tauri::command]
pub async fn list_collections(
    state: tauri::State<'_, Database>,
) -> Result<Vec<CollectionView>, String> {
    state
        .list_collections()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn create_collection(
    name: String,
    description: String,
    state: tauri::State<'_, Database>,
) -> Result<CollectionView, String> {
    state
        .create_collection(&name, &description)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn update_collection(
    collection_id: String,
    patch: CollectionPatchInput,
    state: tauri::State<'_, Database>,
) -> Result<CollectionView, String> {
    state
        .update_collection(&collection_id, &patch)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn delete_collection(
    collection_id: String,
    state: tauri::State<'_, Database>,
) -> Result<(), String> {
    state
        .delete_collection(&collection_id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn list_collection_items(
    collection_id: String,
    state: tauri::State<'_, Database>,
) -> Result<Vec<CollectionItemView>, String> {
    state
        .list_collection_items(&collection_id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn add_collection_item(
    input: CollectionItemInput,
    state: tauri::State<'_, Database>,
) -> Result<CollectionItemView, String> {
    state
        .add_collection_item(&input)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn update_collection_item_note(
    item_id: String,
    patch: CollectionItemPatchInput,
    state: tauri::State<'_, Database>,
) -> Result<CollectionItemView, String> {
    let note = patch.note.unwrap_or_default();
    state
        .update_collection_item_note(&item_id, &note)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn remove_collection_item(
    item_id: String,
    state: tauri::State<'_, Database>,
) -> Result<(), String> {
    state
        .remove_collection_item(&item_id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn export_collection_markdown(
    collection_id: String,
    path: String,
    state: tauri::State<'_, Database>,
) -> Result<String, String> {
    let markdown = state
        .export_collection_markdown(&collection_id)
        .await
        .map_err(|error| error.to_string())?;
    std::fs::write(&path, markdown).map_err(|error| error.to_string())?;
    Ok(path)
}

#[tauri::command]
pub async fn list_tags(state: tauri::State<'_, Database>) -> Result<Vec<TagView>, String> {
    state.list_tags().await.map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn list_target_tags(
    target_type: String,
    target_id: String,
    state: tauri::State<'_, Database>,
) -> Result<Vec<TagView>, String> {
    state
        .list_target_tags(&target_type, &target_id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn create_tag(
    name: String,
    color: String,
    state: tauri::State<'_, Database>,
) -> Result<TagView, String> {
    state
        .create_tag(&name, &color)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn update_tag(
    tag_id: String,
    patch: TagPatchInput,
    state: tauri::State<'_, Database>,
) -> Result<TagView, String> {
    state
        .update_tag(&tag_id, &patch)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn delete_tag(tag_id: String, state: tauri::State<'_, Database>) -> Result<(), String> {
    state
        .delete_tag(&tag_id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn add_tag_to_target(
    target_type: String,
    target_id: String,
    name: String,
    color: String,
    state: tauri::State<'_, Database>,
) -> Result<TagView, String> {
    state
        .add_tag_to_target(&target_type, &target_id, &name, &color)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn remove_tag_from_target(
    target_type: String,
    target_id: String,
    tag_id: String,
    state: tauri::State<'_, Database>,
) -> Result<(), String> {
    state
        .remove_tag_from_target(&target_type, &target_id, &tag_id)
        .await
        .map_err(|error| error.to_string())
}
