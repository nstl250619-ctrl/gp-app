// 登录门面背景层（§16）：深色渐变 + 网格 + 节点网络 SVG + 双光晕 + 池水 + 数据流粒子。
import { useMemo } from 'react';
import workbuddyIcon from '@/assets/tools/workbuddy.png';
import codebuddyIcon from '@/assets/tools/codebuddy.png';
import dshIcon from '@/assets/tools/dsh.png';
import codexIcon from '@/assets/tools/codex.png';
import claudeIcon from '@/assets/tools/claude.png';
import openclawIcon from '@/assets/tools/openclaw.png';
import hermesIcon from '@/assets/tools/hermes.png';

// 与 CSS 修饰类 .login-screen__particle--1/--2/--3 对齐（避免 --0 无样式、蓝色粒子缺失）
const PARTICLE_VARIANTS = [1, 2, 3] as const;

// 平台节点（绿池中转连接的 7 个 AI 工具）：官方图标（与配置页同源，尺寸为其两倍）+ 微光圆环 + 错落漂浮
const NODES = [
  { name: 'WorkBuddy', icon: workbuddyIcon, x: 240, y: 180, delay: '0s', dur: '10s' },
  { name: 'CodeBuddy', icon: codebuddyIcon, x: 940, y: 150, delay: '1.4s', dur: '11s' },
  { name: 'DeepSeek Harness', icon: dshIcon, x: 600, y: 92, delay: '2.8s', dur: '9s' },
  { name: 'Codex', icon: codexIcon, x: 1030, y: 480, delay: '4.2s', dur: '12s' },
  { name: 'Claude Code', icon: claudeIcon, x: 210, y: 520, delay: '0.7s', dur: '10.5s' },
  { name: 'OpenClaw', icon: openclawIcon, x: 320, y: 660, delay: '2.1s', dur: '11.5s' },
  { name: 'Hermes', icon: hermesIcon, x: 880, y: 680, delay: '3.5s', dur: '9.5s' },
] as const;

// 图标边长：配置面板 44px 的两倍
const ICON_SIZE = 88;

export default function LoginBackdrop() {
  // 数据流粒子（一次性生成；颜色走 CSS 修饰类，避免硬编码 hex）
  const particles = useMemo(
    () =>
      Array.from({ length: 26 }, (_, i) => ({
        left: Math.random() * 100,
        top: 30 + Math.random() * 70,
        size: 2 + Math.random() * 3,
        variant: PARTICLE_VARIANTS[i % PARTICLE_VARIANTS.length],
        duration: 6 + Math.random() * 9,
        delay: Math.random() * 8,
      })),
    [],
  );

  const coreStyle = { fill: 'var(--login-node-green)' } as const;
  const labelStyle = { fill: 'rgba(255,255,255,0.38)', fontSize: 9 } as const;

  return (
    <div className="login-screen__bg">
      <div className="login-screen__grid" />
      <svg className="login-screen__net" viewBox="0 0 1200 800" preserveAspectRatio="xMidYMid slice">
        <defs>
          <linearGradient id="loginLinkGrad" x1="0" y1="0" x2="1" y2="1">
            <stop offset="0" style={{ stopColor: 'var(--login-node-green)' }} />
            <stop offset="1" style={{ stopColor: 'var(--login-node-blue)' }} />
          </linearGradient>
          <linearGradient id="nodeGrad" x1="0" y1="0" x2="1" y2="1">
            <stop offset="0" style={{ stopColor: 'var(--login-node-blue)' }} />
            <stop offset="1" style={{ stopColor: 'var(--login-node-green)' }} />
          </linearGradient>
        </defs>
        <path d="M600 400 L240 180" stroke="url(#loginLinkGrad)" strokeWidth="1.1" fill="none" opacity="0.5" />
        <path d="M600 400 L940 150" stroke="url(#loginLinkGrad)" strokeWidth="1.1" fill="none" opacity="0.5" />
        <path d="M600 400 L600 92" stroke="url(#loginLinkGrad)" strokeWidth="1.1" fill="none" opacity="0.5" />
        <path d="M600 400 L1030 480" stroke="url(#loginLinkGrad)" strokeWidth="1.1" fill="none" opacity="0.5" />
        <path d="M600 400 L210 520" stroke="url(#loginLinkGrad)" strokeWidth="1.1" fill="none" opacity="0.5" />
        <path d="M600 400 L320 660" stroke="url(#loginLinkGrad)" strokeWidth="1.1" fill="none" opacity="0.5" />
        <path d="M600 400 L880 680" stroke="url(#loginLinkGrad)" strokeWidth="1.1" fill="none" opacity="0.5" />
        {/* GreenPool core */}
        <circle cx="600" cy="400" r="46" style={coreStyle} opacity="0.16" />
        <circle cx="600" cy="400" r="16" style={coreStyle} />
        <text x="600" y="435" textAnchor="middle" style={labelStyle}>GreenPool</text>
        {/* Platform nodes: glow ring + official icon + float */}
        {NODES.map((n) => (
          <g key={n.name} className="net-node" style={{ animationDelay: n.delay, animationDuration: n.dur }}>
            <circle cx={n.x} cy={n.y} r={ICON_SIZE / 2 + 10} className="node-ring" />
            <circle cx={n.x} cy={n.y} r={ICON_SIZE / 2 + 10} className="node-ring node-ring--spin" />
            <circle cx={n.x} cy={n.y} r={ICON_SIZE / 2 + 2} fill="url(#nodeGrad)" opacity="0.9" />
            <image
              x={n.x - ICON_SIZE / 2}
              y={n.y - ICON_SIZE / 2}
              width={ICON_SIZE}
              height={ICON_SIZE}
              href={n.icon}
              preserveAspectRatio="xMidYMid meet"
            />
            <text x={n.x} y={n.y + ICON_SIZE / 2 + 18} textAnchor="middle" style={labelStyle}>{n.name}</text>
          </g>
        ))}
      </svg>
      <div className="login-screen__orb login-screen__orb--green" />
      <div className="login-screen__orb login-screen__orb--blue" />
      <div className="login-screen__pool" />
      {particles.map((p, i) => (
        <span
          key={i}
          className={`login-screen__particle login-screen__particle--${p.variant}`}
          style={{
            left: `${p.left}%`,
            top: `${p.top}%`,
            width: p.size,
            height: p.size,
            animationDuration: `${p.duration}s`,
            animationDelay: `${p.delay}s`,
          }}
        />
      ))}
    </div>
  );
}
