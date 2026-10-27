use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use iced::advanced::layout;
use iced::advanced::renderer;
use iced::advanced::text;
use iced::advanced::widget::{Tree, tree};
use iced::advanced::{Layout, Widget};
use iced::{Element, Length, Point, Rectangle, Size, alignment, mouse};

pub const WIDTH: f32 = 260.0;
pub const TICK: Duration = Duration::from_millis(33);

const START_PAUSE: Duration = Duration::from_secs(2);
const END_PAUSE: Duration = Duration::from_secs(2);
const SPEED: f32 = 30.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    StartPause,
    Scrolling,
    EndPause,
}

#[derive(Debug)]
pub struct State {
    offset: f32,
    phase: Phase,
    elapsed: Duration,
    measured_width: Rc<Cell<f32>>,
    animating: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            offset: 0.0,
            phase: Phase::StartPause,
            elapsed: Duration::ZERO,
            measured_width: Rc::new(Cell::new(0.0)),
            animating: false,
        }
    }
}

impl State {
    pub fn set_track(&mut self, track: &str) {
        self.offset = 0.0;
        self.phase = Phase::StartPause;
        self.elapsed = Duration::ZERO;
        self.measured_width.set(0.0);
        self.animating = !track.is_empty();
    }

    pub fn needs_animation(&self) -> bool {
        self.animating
    }

    pub fn tick(&mut self) {
        if !self.animating {
            return;
        }

        let width = self.measured_width.get();

        // The text may not have been laid out yet. Keep waiting for a
        // measurement instead of assuming that the track fits.
        if width == 0.0 {
            return;
        }

        if width <= WIDTH {
            self.offset = 0.0;
            self.phase = Phase::StartPause;
            self.elapsed = Duration::ZERO;
            self.animating = false;
            return;
        }

        let max_offset = width - WIDTH;
        let elapsed = TICK.as_secs_f32();
        self.elapsed += TICK;

        match self.phase {
            Phase::StartPause => {
                if self.elapsed >= START_PAUSE {
                    self.phase = Phase::Scrolling;
                    self.elapsed = Duration::ZERO;
                }
            }
            Phase::Scrolling => {
                self.offset = (self.offset + SPEED * elapsed).min(max_offset);

                if self.offset >= max_offset {
                    self.phase = Phase::EndPause;
                    self.elapsed = Duration::ZERO;
                }
            }
            Phase::EndPause => {
                if self.elapsed >= END_PAUSE {
                    self.offset = 0.0;
                    self.phase = Phase::StartPause;
                    self.elapsed = Duration::ZERO;
                }
            }
        }
    }
}

pub fn widget<'a, Message: 'a>(content: &'a str, state: &State) -> Element<'a, Message> {
    Element::new(Marquee {
        content,
        width: WIDTH,
        offset: state.offset,
        measured_width: Rc::clone(&state.measured_width),
    })
}

struct Marquee<'a> {
    content: &'a str,
    width: f32,
    offset: f32,
    measured_width: Rc<Cell<f32>>,
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer> for Marquee<'_>
where
    Renderer: text::Renderer,
{
    fn size(&self) -> Size<Length> {
        Size {
            width: Length::Fixed(self.width),
            height: Length::Shrink,
        }
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<text::paragraph::Plain<Renderer::Paragraph>>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(text::paragraph::Plain::<Renderer::Paragraph>::default())
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        _limits: &layout::Limits,
    ) -> layout::Node {
        let state = tree
            .state
            .downcast_mut::<text::paragraph::Plain<Renderer::Paragraph>>();

        let _ = state.update(text::Text {
            content: self.content,
            bounds: Size::INFINITE,
            size: renderer.default_size(),
            line_height: text::LineHeight::default(),
            font: renderer.default_font(),
            align_x: text::Alignment::Left,
            align_y: alignment::Vertical::Top,
            shaping: text::Shaping::default(),
            wrapping: text::Wrapping::None,
        });

        self.measured_width.set(state.min_width());

        layout::Node::new(Size::new(self.width, state.min_height()))
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let state = tree
            .state
            .downcast_ref::<text::paragraph::Plain<Renderer::Paragraph>>();
        let bounds = layout.bounds();

        let Some(clip) = bounds.intersection(viewport) else {
            return;
        };

        let y = bounds.y + (bounds.height - state.min_height()) / 2.0;

        renderer.fill_paragraph(
            state.raw(),
            Point::new(bounds.x - self.offset, y),
            style.text_color,
            clip,
        );
    }
}

impl<'a, Message, Theme, Renderer> From<Marquee<'a>> for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: 'a,
    Renderer: text::Renderer + 'a,
{
    fn from(widget: Marquee<'a>) -> Self {
        Element::new(widget)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn long_track(width: f32) -> State {
        let mut state = State::default();
        state.set_track("Long track");
        state.measured_width.set(width);
        state
    }

    #[test]
    fn waits_for_measurement_before_scrolling() {
        let mut state = long_track(0.0);
        state.tick();

        assert!(state.needs_animation());
        assert_eq!(state.offset, 0.0);
    }

    #[test]
    fn stops_animating_when_track_fits() {
        let mut state = long_track(WIDTH);
        state.tick();

        assert!(!state.needs_animation());
        assert_eq!(state.offset, 0.0);
    }

    #[test]
    fn pauses_before_scrolling() {
        let mut state = long_track(WIDTH + 100.0);

        for _ in 0..60 {
            state.tick();
        }

        assert_eq!(state.offset, 0.0);

        state.tick();
        state.tick();

        assert!(state.offset > 0.0);
    }

    #[test]
    fn pauses_at_the_end_then_restarts() {
        let width = WIDTH + 10.0;
        let mut state = long_track(width);

        for _ in 0..1_000 {
            if state.phase == Phase::EndPause {
                break;
            }

            state.tick();
        }

        assert_eq!(state.phase, Phase::EndPause);
        assert_eq!(state.offset, width - WIDTH);

        for _ in 0..62 {
            state.tick();
        }

        assert_eq!(state.phase, Phase::StartPause);
        assert_eq!(state.offset, 0.0);
    }
}
