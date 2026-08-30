use napi_derive::napi;

/// The Steam Inventory Service (`ISteamInventory`).
///
/// Every request that talks to Steam returns a promise that resolves with the
/// items the finished result describes, or rejects with the name of the
/// `EResult` Steam reported (for instance `RateLimitExceeded`), so callers can
/// branch on it.
#[napi]
pub mod inventory {
    use napi::bindgen_prelude::{BigInt, Error};
    use steamworks::{InventoryResult as SteamInventoryResult, ItemDefId, ItemInstanceId};
    use tokio::sync::oneshot;

    #[napi(object)]
    pub struct InventoryItem {
        /// The instance id of the item stack.
        pub item_id: BigInt,
        /// The item definition id.
        pub definition: i32,
        pub quantity: u32,
        /// `ESteamItemFlags` bits: 1 no-trade, 256 removed, 512 consumed.
        pub flags: u32,
    }

    #[napi(object)]
    pub struct InventoryResult {
        /// Server time the result was generated, unix seconds.
        pub timestamp: u32,
        pub items: Vec<InventoryItem>,
    }

    #[napi(object)]
    pub struct DefinitionQuantity {
        pub definition: i32,
        pub quantity: u32,
    }

    #[napi(object)]
    pub struct InstanceQuantity {
        pub item_id: BigInt,
        pub quantity: u32,
    }

    fn instance(id: &BigInt) -> ItemInstanceId {
        ItemInstanceId(id.get_u64().1)
    }

    fn finish(
        result: Result<SteamInventoryResult, steamworks::SteamError>,
    ) -> Result<InventoryResult, Error> {
        match result {
            Ok(result) => Ok(InventoryResult {
                timestamp: result.timestamp(),
                items: result
                    .items()
                    .into_iter()
                    .map(|item| InventoryItem {
                        item_id: BigInt::from(item.item_id.0),
                        definition: item.definition.0,
                        quantity: item.quantity as u32,
                        flags: item.flags.0 as u32,
                    })
                    .collect(),
            }),
            Err(e) => Err(Error::from_reason(format!("{:?}", e))),
        }
    }

    async fn request(
        submit: impl FnOnce(
            steamworks::Inventory,
            Box<dyn FnOnce(Result<SteamInventoryResult, steamworks::SteamError>) + Send>,
        ),
    ) -> Result<InventoryResult, Error> {
        let client = crate::client::get_client();
        let (tx, rx) = oneshot::channel();
        submit(
            client.inventory(),
            Box::new(move |result| {
                let _ = tx.send(result);
            }),
        );
        finish(
            rx.await
                .map_err(|_| Error::from_reason("inventory request dropped"))?,
        )
    }

    /// {@link https://partner.steamgames.com/doc/api/ISteamInventory#GetAllItems}
    #[napi]
    pub async fn get_all_items() -> Result<InventoryResult, Error> {
        request(|inv, cb| inv.get_all_items(cb)).await
    }

    /// {@link https://partner.steamgames.com/doc/api/ISteamInventory#GetItemsByID}
    #[napi]
    pub async fn get_items_by_id(item_ids: Vec<BigInt>) -> Result<InventoryResult, Error> {
        let ids: Vec<ItemInstanceId> = item_ids.iter().map(instance).collect();
        request(move |inv, cb| inv.get_items_by_id(&ids, cb)).await
    }

    /// {@link https://partner.steamgames.com/doc/api/ISteamInventory#ConsumeItem}
    #[napi]
    pub async fn consume_item(item_id: BigInt, quantity: u32) -> Result<InventoryResult, Error> {
        let id = instance(&item_id);
        request(move |inv, cb| inv.consume_item(id, quantity, cb)).await
    }

    /// {@link https://partner.steamgames.com/doc/api/ISteamInventory#ExchangeItems}
    #[napi]
    pub async fn exchange_items(
        generate: Vec<DefinitionQuantity>,
        destroy: Vec<InstanceQuantity>,
    ) -> Result<InventoryResult, Error> {
        let generate: Vec<(ItemDefId, u32)> = generate
            .iter()
            .map(|g| (ItemDefId(g.definition), g.quantity))
            .collect();
        let destroy: Vec<(ItemInstanceId, u32)> = destroy
            .iter()
            .map(|d| (instance(&d.item_id), d.quantity))
            .collect();
        request(move |inv, cb| inv.exchange_items(&generate, &destroy, cb)).await
    }

    /// Developer-only. {@link https://partner.steamgames.com/doc/api/ISteamInventory#GenerateItems}
    #[napi]
    pub async fn generate_items(items: Vec<DefinitionQuantity>) -> Result<InventoryResult, Error> {
        let items: Vec<(ItemDefId, u32)> = items
            .iter()
            .map(|g| (ItemDefId(g.definition), g.quantity))
            .collect();
        request(move |inv, cb| inv.generate_items(&items, cb)).await
    }

    /// {@link https://partner.steamgames.com/doc/api/ISteamInventory#TransferItemQuantity}
    #[napi]
    pub async fn transfer_item_quantity(
        source: BigInt,
        quantity: u32,
        dest: Option<BigInt>,
    ) -> Result<InventoryResult, Error> {
        let source = instance(&source);
        let dest = dest.as_ref().map(instance);
        request(move |inv, cb| inv.transfer_item_quantity(source, quantity, dest, cb)).await
    }

    /// {@link https://partner.steamgames.com/doc/api/ISteamInventory#TriggerItemDrop}
    #[napi]
    pub async fn trigger_item_drop(definition: i32) -> Result<InventoryResult, Error> {
        request(move |inv, cb| inv.trigger_item_drop(ItemDefId(definition), cb)).await
    }

    /// {@link https://partner.steamgames.com/doc/api/ISteamInventory#GrantPromoItems}
    #[napi]
    pub async fn grant_promo_items() -> Result<InventoryResult, Error> {
        request(|inv, cb| inv.grant_promo_items(cb)).await
    }

    /// {@link https://partner.steamgames.com/doc/api/ISteamInventory#AddPromoItems}
    #[napi]
    pub async fn add_promo_items(definitions: Vec<i32>) -> Result<InventoryResult, Error> {
        let defs: Vec<ItemDefId> = definitions.into_iter().map(ItemDefId).collect();
        request(move |inv, cb| inv.add_promo_items(&defs, cb)).await
    }

    /// {@link https://partner.steamgames.com/doc/api/ISteamInventory#SendItemDropHeartbeat}
    #[napi]
    pub fn send_item_drop_heartbeat() {
        crate::client::get_client()
            .inventory()
            .send_item_drop_heartbeat()
    }

    /// Starts loading the item definitions; the `SteamInventoryDefinitionUpdate`
    /// callback fires when they are available.
    /// {@link https://partner.steamgames.com/doc/api/ISteamInventory#LoadItemDefinitions}
    #[napi]
    pub fn load_item_definitions() -> bool {
        crate::client::get_client()
            .inventory()
            .load_item_definitions()
    }

    /// The ids of every loaded item definition, or `null` before they load.
    /// {@link https://partner.steamgames.com/doc/api/ISteamInventory#GetItemDefinitionIDs}
    #[napi]
    pub fn get_item_definition_ids() -> Option<Vec<i32>> {
        crate::client::get_client()
            .inventory()
            .item_definition_ids()
            .map(|ids| ids.into_iter().map(|id| id.0).collect())
    }

    /// A property of a loaded definition; with no name, the comma-separated
    /// property names.
    /// {@link https://partner.steamgames.com/doc/api/ISteamInventory#GetItemDefinitionProperty}
    #[napi]
    pub fn get_item_definition_property(definition: i32, name: Option<String>) -> Option<String> {
        crate::client::get_client()
            .inventory()
            .item_definition_property(ItemDefId(definition), name.as_deref())
    }
}
