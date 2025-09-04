use basel::Theme;

fn main() {
    let themes = Theme::themes();
    for theme in themes {
        println!("{theme:?}")
    }
}
