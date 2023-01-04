use bevy::prelude::*;

#[derive(Resource, Deref)]
pub struct UserName(pub String);
