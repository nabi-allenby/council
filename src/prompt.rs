use std::fs;
use std::path::Path;

use crate::error::CouncilError;

fn load(prompts_dir: &Path, name: &str) -> Result<String, CouncilError> {
    let path = prompts_dir.join(name);
    fs::read_to_string(&path)
        .map(|s| s.trim().to_string())
        .map_err(|_| {
            CouncilError::FileNotFound(format!("Prompt file not found: {}", path.display()))
        })
}

pub fn discussion_prompt(
    prompts_dir: &Path,
    round_num: u32,
    total_rounds: u32,
) -> Result<String, CouncilError> {
    let round_file = format!("round-{}.md", round_num.min(3));
    let round_guidance = load(prompts_dir, &round_file)?;
    let engagement = load(prompts_dir, "engagement.md")?;
    let brevity = load(prompts_dir, "brevity.md")?;
    let response_format = load(prompts_dir, "response-format.md")?;

    Ok(format!(
        "You are participating in a council discussion, Round {} of {}.\n\n{}\n\n{}\n\n{}\n\n{}",
        round_num, total_rounds, round_guidance, engagement, brevity, response_format
    ))
}

pub fn vote_prompt(prompts_dir: &Path, question: &str) -> Result<String, CouncilError> {
    let template = load(prompts_dir, "vote.md")?;
    let engagement = load(prompts_dir, "engagement.md")?;
    let vote_format = load(prompts_dir, "vote-format.md")?;

    Ok(format!(
        "{}\n\n{}\n\n{}",
        template.replace("{question}", question),
        engagement,
        vote_format
    ))
}
