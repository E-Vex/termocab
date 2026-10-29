mod content;
mod progress;
mod engine;

fn main() {
    let stages = match content::load_language("languages/french/stages.json") {
        Ok(stages) => stages,
        Err(e) => {
            eprintln!("termocab: failed to load French content: {e}");
            std::process::exit(1);
        }
    };

    println!("termocab — language learning for terminal enthusiasts");
    println!();
    println!("French: {} stage(s) loaded", stages.len());

    for stage in &stages {
        println!(
            "  Stage {}: \"{}\" [{}] — {} words, {} exercises",
            stage.id,
            stage.title,
            stage.level_hint,
            stage.vocabulary.len(),
            stage.exercises.len()
        );
    }
}
