use crate::error::ApiError;
use crate::types::item::Item as ItemType;

pub struct Regular;
pub struct Mod {
    rank: u32
}
pub struct Sculpture {
    amber_stars: u32,
    cyan_stars: u32,
    base_endo: u32,
    endo_multiplier: f32,
}

pub struct Item<State = Regular> {
    object: ItemType,
    state: State,
}

impl<State> Item<State> {
    pub fn get_type(&self) -> ItemType {
        self.object.clone()
    }
    
    pub fn get_slug(&self) -> String {
        self.object.slug.clone()
    }
}

impl Item<Regular> {
    pub fn new(object: &ItemType) -> Self {
        Item {
            object: object.clone(),
            state: Regular,
        }
    }
    
    pub fn to_sculpture(&self) -> Result<Item<Sculpture>, ApiError> {
        let cyan_stars = if let Some(cyan) = self.object.max_cyan_stars {
            cyan
        } else { 0 };
        let amber_stars = if let Some(amber) = self.object.max_amber_stars {
            amber
        } else { 0 };
        
        if let (Some(base_endo), Some(endo_multiplier)) = (
            self.object.base_endo, self.object.endo_multiplier) {
            Ok(Item {
                object: self.object.clone(),
                state: Sculpture {
                    amber_stars,
                    cyan_stars,
                    base_endo,
                    endo_multiplier,
                }
            })
        } else { 
            Err(ApiError::ParsingError(String::from("Item is not an Ayatan Sculpture")))
        }
    }
    
    pub fn is_sculpture(&self) -> bool {
        self.object.base_endo.is_some() && self.object.endo_multiplier.is_some()
    }

    pub fn to_mod(&self) -> Result<Item<Mod>, ApiError> {
        if let Some(rank) = self.object.max_rank {
            Ok(Item {
                object: self.object.clone(),
                state: Mod {
                    rank,
                }
            })
        } else { 
            Err(ApiError::ParsingError(String::from("Item is not a Mod")))
        }
    }
    
    pub fn is_mod(&self) -> bool {
        self.object.max_rank.is_some()
    }
}

impl Item<Sculpture> {
    /**
    Calculate the value of an Ayatan Sculpture based on installed Ayatan Stars
    
    # Arguments
    - `cyan_stars`: Number of installed Cyan Stars, a value of None uses the max value
    - `amber_stars`: Number of installed Amber Stars, a value of None uses the max value
    
    # Returns
    The total endo value of a sculpture with defined amount of stars installed
    */
    pub fn calculate_value(&self, cyan_stars: Option<u32>, amber_stars: Option<u32>) -> u32 {
        let base: f32 = self.state.base_endo as f32;
        let multiplier = self.state.endo_multiplier;
        let sockets = self.state.amber_stars + self.state.cyan_stars;
        
        let cyan = if cyan_stars.is_some() {
            cyan_stars.unwrap()
        } else { 
            self.state.cyan_stars
        };
        
        let amber = if amber_stars.is_some() {
            amber_stars.unwrap()
        } else { 
            self.state.amber_stars
        };
        
        if sockets == 0 {
            panic!("Ayatan Sculpture has an invalid amount of sockets");
        }

        let total_stars = (cyan + amber) as f32;
        let base_part = base + 50.0 * (cyan as f32) + 100.0 * (amber as f32);
        let socket_factor = 1.0 + multiplier * total_stars / (sockets as f32);

        (base_part * socket_factor) as u32
    }
}
