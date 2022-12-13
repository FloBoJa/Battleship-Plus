use bevy::prelude::{App, Plugin};
use bevy_quinnet::client::QuinnetClientPlugin;

pub struct NetworkingPlugin;

impl Plugin for NetworkingPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugin(QuinnetClientPlugin {});
    }
}
