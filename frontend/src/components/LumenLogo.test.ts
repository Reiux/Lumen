import { describe, expect, it } from 'vitest'
import { mount } from '@vue/test-utils'
import LumenLogo from './LumenLogo.vue'

describe('LumenLogo', () => {
  it('renders transparent SVG with viewBox 0 0 108 108 and no background rect', () => {
    const wrapper = mount(LumenLogo, {
      props: { size: 40 },
    })

    const svg = wrapper.find('svg')
    expect(svg.exists()).toBe(true)
    expect(svg.attributes('viewBox')).toBe('0 0 108 108')

    // No background box/rect
    expect(wrapper.find('rect').exists()).toBe(false)
    expect(wrapper.classes()).toContain('bg-transparent')
  })

  it('renders four blade groups with SVG attribute transforms and no inline px transform-origin', () => {
    const wrapper = mount(LumenLogo, {
      props: { size: 160 },
    })

    // Find all <g transform="rotate(...)"> inside the aperture group
    const bladeGroups = wrapper.findAll('svg g.aperture-group > g[transform]')
    expect(bladeGroups.length).toBe(4)

    // Assert exact attribute transforms with SVG coordinate pivot (54 54)
    expect(bladeGroups[0].attributes('transform')).toBe('rotate(0 54 54)')
    expect(bladeGroups[1].attributes('transform')).toBe('rotate(90 54 54)')
    expect(bladeGroups[2].attributes('transform')).toBe('rotate(180 54 54)')
    expect(bladeGroups[3].attributes('transform')).toBe('rotate(270 54 54)')

    // Assert no inline px transform-origin on any blade group
    bladeGroups.forEach((blade) => {
      const inlineStyle = blade.attributes('style') || ''
      expect(inlineStyle).not.toMatch(/transform-origin:\s*\d+px/i)
      expect(blade.attributes('transform-origin')).toBeUndefined()
    })

    // Assert blade colors match dynamic theme tokens with brand fallbacks
    const paths = wrapper.findAll('svg g.aperture-group > g[transform] path')
    expect(paths.length).toBe(4)
    expect(paths[0].attributes('fill')).toBe('var(--md-sys-color-primary, #38BDF8)')
    expect(paths[1].attributes('fill')).toBe('var(--md-sys-color-secondary, #60A5FA)')
    expect(paths[2].attributes('fill')).toBe('var(--md-sys-color-tertiary, #818CF8)')
    expect(paths[3].attributes('fill')).toBe('var(--md-sys-color-primary-container, #A78BFA)')

    // Assert central radiant lens core
    const core = wrapper.find('svg circle')
    expect(core.exists()).toBe(true)
    expect(core.attributes('cx')).toBe('54')
    expect(core.attributes('cy')).toBe('54')
    expect(core.attributes('r')).toBe('7')
    expect(core.attributes('fill')).toBe('var(--md-sys-color-on-surface, #FFFFFF)')
  })

  it('applies custom blade colors when provided as props', () => {
    const wrapper = mount(LumenLogo, {
      props: {
        blade1Color: '#FF0000',
        blade2Color: '#00FF00',
        blade3Color: '#0000FF',
        blade4Color: '#FFFF00',
        centerColor: '#000000',
      },
    })

    const paths = wrapper.findAll('svg g.aperture-group > g[transform] path')
    expect(paths[0].attributes('fill')).toBe('#FF0000')
    expect(paths[1].attributes('fill')).toBe('#00FF00')
    expect(paths[2].attributes('fill')).toBe('#0000FF')
    expect(paths[3].attributes('fill')).toBe('#FFFF00')
    expect(wrapper.find('svg circle').attributes('fill')).toBe('#000000')
  })

  it('applies is-launching animation class only to wrapper group when animated prop is true', () => {
    const wrapper = mount(LumenLogo, {
      props: { size: 160, animated: true },
    })

    const wrapperGroup = wrapper.find('.aperture-group')
    expect(wrapperGroup.exists()).toBe(true)
    expect(wrapperGroup.classes()).toContain('is-launching')
  })
})
