//! Phrase planner: form templates assign a role to every bar.

use clap::ValueEnum;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, ValueEnum)]
pub enum Template {
    /// Pick by length: Sentence for 8, AABA for 16+.
    #[default]
    Auto,
    /// basic idea (2) + repetition (2) + continuation (2) + cadence (2)
    Sentence,
    /// antecedent (4, half cadence) + consequent (4, full cadence)
    Period,
    /// A A B A, 8 bars each (16+ bars)
    Aaba,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cadence {
    Half,
    Full,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// New material.
    Idea,
    /// A (varied) repeat of bar `of`.
    Repeat { of: u32 },
    /// A fragment / sequence built from bar `of`.
    Fragment { of: u32 },
    /// Contrasting free material.
    Free,
    /// Phrase end.
    Cadence(Cadence),
}

#[derive(Clone, Debug)]
pub struct Form {
    pub template: Template,
    pub roles: Vec<Role>,
}

impl Form {
    pub fn plan(template: Template, bars: u32) -> Form {
        let template = match template {
            Template::Auto if bars >= 16 => Template::Aaba,
            Template::Auto => Template::Sentence,
            t => t,
        };
        let mut roles = Vec::with_capacity(bars as usize);
        match template {
            Template::Sentence => {
                for b in 0..bars {
                    roles.push(sentence_role(b, bars));
                }
            }
            Template::Period => {
                for b in 0..bars {
                    roles.push(period_role(b, bars));
                }
            }
            Template::Aaba => {
                for b in 0..bars {
                    let section = (b / 8) % 4;
                    let local = b % 8;
                    let role = match section {
                        // A: a sentence.
                        0 => sentence_role(local, 8),
                        // A again: repeat bars 0..8 with variation.
                        1 | 3 => {
                            if local == 7 {
                                Role::Cadence(Cadence::Full)
                            } else {
                                Role::Repeat { of: b - 8 * section }
                            }
                        }
                        // B: new sentence.
                        _ => sentence_role(local, 8),
                    };
                    roles.push(role);
                }
            }
            Template::Auto => unreachable!(),
        }
        // The very last bar is always the final cadence.
        if let Some(last) = roles.last_mut() {
            *last = Role::Cadence(Cadence::Full);
        }
        Form { template, roles }
    }

    pub fn role(&self, bar: u32) -> Role {
        self.roles[bar as usize]
    }

    /// Bars that end a phrase (any cadence).
    pub fn phrase_ends(&self) -> Vec<u32> {
        self.roles
            .iter()
            .enumerate()
            .filter(|(_, r)| matches!(r, Role::Cadence(_)))
            .map(|(i, _)| i as u32)
            .collect()
    }

    /// Bars ending an antecedent (half cadence).
    pub fn half_cadences(&self) -> Vec<u32> {
        self.roles
            .iter()
            .enumerate()
            .filter(|(_, r)| matches!(r, Role::Cadence(Cadence::Half)))
            .map(|(i, _)| i as u32)
            .collect()
    }

    /// First bar of the phrase containing `bar`.
    pub fn phrase_start(&self, bar: u32) -> u32 {
        (0..bar)
            .rev()
            .find(|&b| matches!(self.roles[b as usize], Role::Cadence(_)))
            .map(|b| b + 1)
            .unwrap_or(0)
    }
}

fn sentence_role(b: u32, bars: u32) -> Role {
    let local = b % 8;
    match local {
        0 | 1 => Role::Idea,
        2 => Role::Repeat { of: b - 2 },
        3 => {
            // 4-bar sentence phrase: the first half ends open.
            if bars > 4 { Role::Cadence(Cadence::Half) } else { Role::Cadence(Cadence::Full) }
        }
        4 | 5 => Role::Fragment { of: b - local },
        6 => Role::Free,
        _ => Role::Cadence(Cadence::Full),
    }
}

fn period_role(b: u32, _bars: u32) -> Role {
    let local = b % 8;
    match local {
        0 | 1 => Role::Idea,
        2 => Role::Free,
        3 => Role::Cadence(Cadence::Half),
        4 | 5 => Role::Repeat { of: b - 4 },
        6 => Role::Free,
        _ => Role::Cadence(Cadence::Full),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sentence_8() {
        let f = Form::plan(Template::Auto, 8);
        assert_eq!(f.template, Template::Sentence);
        assert_eq!(f.role(0), Role::Idea);
        assert_eq!(f.role(2), Role::Repeat { of: 0 });
        assert_eq!(f.role(3), Role::Cadence(Cadence::Half));
        assert_eq!(f.role(4), Role::Fragment { of: 0 });
        assert_eq!(f.role(7), Role::Cadence(Cadence::Full));
        assert_eq!(f.phrase_ends(), vec![3, 7]);
        assert_eq!(f.phrase_start(5), 4);
    }

    #[test]
    fn period_and_aaba() {
        let f = Form::plan(Template::Period, 8);
        assert_eq!(f.role(4), Role::Repeat { of: 0 });
        assert_eq!(f.half_cadences(), vec![3]);
        let f = Form::plan(Template::Auto, 32);
        assert_eq!(f.template, Template::Aaba);
        assert_eq!(f.role(8), Role::Repeat { of: 0 });
        assert_eq!(f.role(16), Role::Idea);
        assert_eq!(f.role(24), Role::Repeat { of: 0 });
        assert_eq!(f.role(31), Role::Cadence(Cadence::Full));
    }

    #[test]
    fn odd_lengths_end_with_full_cadence() {
        for bars in [1, 4, 5, 12] {
            let f = Form::plan(Template::Auto, bars);
            assert_eq!(f.roles.len(), bars as usize);
            assert_eq!(f.role(bars - 1), Role::Cadence(Cadence::Full));
        }
    }
}
