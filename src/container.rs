use bollard::container::Stats;

pub struct Container {
    pub name: String,
}

impl Container {
    pub fn from_stats(stats: &Stats) -> Self {
        Container {
            name: stats.name.clone(),
        }
    }

    pub fn update_from_stats(&mut self, stats: &Stats) {
        if self.name != stats.name {
            log::warn!(
                "Container '{}' wrongly received stats update for container '{}'",
                self.name,
                stats.name
            )
        } else {
            log::debug!("Updating Container '{}' from stats", self.name);
            // TODO: Update stats - and don't update the name since it doesn't actually change
        }
    }
}
