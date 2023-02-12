use bevy::prelude::*;
use battleship_plus_common::messages;


use crate::networking;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_startup_system(test)
        ;
    }
}

fn test() {
    println!("ok");
}
